// 插件市场：拉取 index.json（多源容灾）→ 渲染列表 → 下载安装（走 installer 统一管线）。
// 索引格式见插件仓库 README；应用只认 index.json，不关心仓库内部结构。
import { invoke } from '@tauri-apps/api/core';
import { installFromBytesPublic } from './host';
import { MARKET_SOURCES, readMarketSource } from './config';
import type { PluginManifest } from './types';

export interface MarketEntry {
  id: string;
  name: string;
  version: string;
  type: 'view' | 'protocol';
  description?: string;
  author?: string;
  download: string; // 绝对 URL（建议固定到 commit 的不可变链接）
  sha256?: string;
  size?: number;
  minAppVersion?: string;
  apiVersion?: number;
}

export interface MarketIndex {
  apiVersion: number;
  updatedAt?: string;
  plugins: MarketEntry[];
}

// 网络与体积限制：索引是小文件（10s / 1MB），zip 安装包较大（60s / 32MB）。
// 超限立即中止请求，避免网络卡死或超大响应把内存打爆。
const INDEX_TIMEOUT_MS = 10_000;
const INDEX_MAX_BYTES = 1024 * 1024;
const ZIP_TIMEOUT_MS = 60_000;
const ZIP_MAX_BYTES = 32 * 1024 * 1024;

const SHA256_RE = /^[a-f0-9]{64}$/i;

const formatSize = (bytes: number) => `${Math.round(bytes / 1024 / 1024)}MB`;

// 带超时 + 体积上限的 GET：分块读取并累计，超限立即 abort（不把超大响应一次性读进内存）
const fetchLimited = async (url: string, timeoutMs: number, maxBytes: number, label: string): Promise<Uint8Array> => {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  const timeoutError = () => new Error(`${label}请求超时（${timeoutMs / 1000}s），请检查网络后重试`);
  let limitHit = false;
  try {
    let res: Response;
    try {
      res = await fetch(url, { cache: 'no-store', signal: controller.signal });
    } catch (e) {
      if (controller.signal.aborted) throw timeoutError();
      throw new Error(`${label}下载失败：${e instanceof Error ? e.message : e}`);
    }
    if (!res.ok) throw new Error(`${label}下载失败：HTTP ${res.status}`);
    // 有 Content-Length 时提前拒绝，避免白下载一遍
    const declared = Number(res.headers.get('content-length') ?? NaN);
    if (Number.isFinite(declared) && declared > maxBytes) {
      limitHit = true;
      controller.abort();
      throw new Error(`${label}体积超出上限（${formatSize(maxBytes)}），已取消`);
    }
    if (!res.body) {
      const all = new Uint8Array(await res.arrayBuffer());
      if (all.length > maxBytes) throw new Error(`${label}体积超出上限（${formatSize(maxBytes)}），已取消`);
      return all;
    }
    const reader = res.body.getReader();
    const chunks: Uint8Array[] = [];
    let total = 0;
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      if (!value || value.length === 0) continue;
      total += value.length;
      if (total > maxBytes) {
        limitHit = true;
        controller.abort(); // 中止底层请求，避免继续占用带宽
        void reader.cancel().catch(() => {});
        throw new Error(`${label}体积超出上限（${formatSize(maxBytes)}），已取消`);
      }
      chunks.push(value);
    }
    const out = new Uint8Array(total);
    let offset = 0;
    for (const chunk of chunks) {
      out.set(chunk, offset);
      offset += chunk.length;
    }
    return out;
  } catch (e) {
    // 读取阶段被超时 abort：转换成可读的超时提示（体积超限已有专门文案，直接透出）
    if (controller.signal.aborted && !limitHit) throw timeoutError();
    throw e;
  } finally {
    clearTimeout(timer);
  }
};

const validEntry = (raw: unknown): MarketEntry | null => {
  if (typeof raw !== 'object' || raw === null) return null;
  const m = raw as Record<string, unknown>;
  const str = (v: unknown) => typeof v === 'string' && v.length > 0;
  if (!str(m.id) || !str(m.name) || !str(m.version) || !str(m.download)) return null;
  return {
    id: String(m.id),
    name: String(m.name),
    version: String(m.version),
    type: m.type === 'view' ? 'view' : 'protocol',
    description: str(m.description) ? String(m.description) : undefined,
    author: str(m.author) ? String(m.author) : undefined,
    download: String(m.download),
    sha256: str(m.sha256) ? String(m.sha256) : undefined,
    size: typeof m.size === 'number' ? m.size : undefined,
    minAppVersion: str(m.minAppVersion) ? String(m.minAppVersion) : undefined,
    apiVersion: typeof m.apiVersion === 'number' ? m.apiVersion : undefined,
  };
};

// 依次尝试：用户自定义源 → jsDelivr → GitHub raw
export const fetchMarketIndex = async (): Promise<MarketIndex> => {
  const sources = [...new Set([readMarketSource(), ...MARKET_SOURCES])];
  let lastErr: unknown = null;
  for (const url of sources) {
    try {
      const bytes = await fetchLimited(url, INDEX_TIMEOUT_MS, INDEX_MAX_BYTES, '插件索引');
      const data = JSON.parse(new TextDecoder().decode(bytes)) as {
        apiVersion?: number;
        updatedAt?: string;
        plugins?: unknown[];
      };
      if (!Array.isArray(data.plugins)) throw new Error('索引缺少 plugins 数组');
      return {
        apiVersion: typeof data.apiVersion === 'number' ? data.apiVersion : 1,
        updatedAt: data.updatedAt,
        plugins: data.plugins.map(validEntry).filter((p): p is MarketEntry => p !== null),
      };
    } catch (e) {
      lastErr = e;
    }
  }
  throw new Error(`插件市场不可达：${lastErr instanceof Error ? lastErr.message : lastErr}`);
};

// 市场安装：应用内直接下载 zip（市场源均支持 CORS），校验后走统一安装管线。
// 市场安装强制 sha256；下载后由安装器核对清单 id/版本/apiVersion/minAppVersion 与索引一致。
export const installFromMarket = async (entry: MarketEntry): Promise<PluginManifest> => {
  if (!entry.sha256 || !SHA256_RE.test(entry.sha256)) {
    throw new Error('插件索引缺少合法的 sha256 校验值，已拒绝安装（索引不完整）');
  }
  const bytes = await fetchLimited(entry.download, ZIP_TIMEOUT_MS, ZIP_MAX_BYTES, '插件包');
  // size 与索引不符只告警不拒绝：size 仅用于展示/预检，完整性由 sha256 保证
  if (entry.size !== undefined && entry.size !== bytes.length) {
    console.warn(`[market] ${entry.id} 索引 size=${entry.size}，实际下载 ${bytes.length} 字节，以实际内容为准`);
  }
  return installFromBytesPublic(bytes, entry.sha256, 'market', {
    id: entry.id,
    version: entry.version,
    apiVersion: entry.apiVersion,
    minAppVersion: entry.minAppVersion,
  });
};

// 本地导入：由后端弹出原生文件对话框并读回 zip 字节（用户知情选择），
// 返回 null 表示用户取消；解压/校验/安装统一走 installer 管线，由 PluginManager 决定提示。
export const importLocalPlugin = async (): Promise<PluginManifest | null> => {
  const picked = await invoke<{ name: string; bytes: number[] } | null>('import_plugin_zip');
  if (!picked) return null;
  const bytes = new Uint8Array(picked.bytes ?? []);
  if (bytes.length === 0) throw new Error('导入的插件包为空');
  return installFromBytesPublic(bytes, undefined, 'local');
};
