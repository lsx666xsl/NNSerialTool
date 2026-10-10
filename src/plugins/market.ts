// 插件市场：拉取 index.json（多源容灾）→ 渲染列表 → 下载安装（走 installer 统一管线）。
// 索引格式见插件仓库 README；应用只认 index.json，不关心仓库内部结构。
import { invoke } from '@tauri-apps/api/core';
import { installFromBytesPublic } from './host';
import { MARKET_SOURCES, readMarketSource } from './config';

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
      const res = await fetch(url, { cache: 'no-store' });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const data = (await res.json()) as { apiVersion?: number; updatedAt?: string; plugins?: unknown[] };
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

// 市场安装：应用内直接下载 zip（市场源均支持 CORS），校验后走统一安装管线
export const installFromMarket = async (entry: MarketEntry): Promise<void> => {
  let res: Response;
  try {
    res = await fetch(entry.download, { cache: 'no-store' });
  } catch (e) {
    throw new Error(`下载插件包失败：${e instanceof Error ? e.message : e}`);
  }
  if (!res.ok) throw new Error(`下载插件包失败：HTTP ${res.status}`);
  const bytes = new Uint8Array(await res.arrayBuffer());
  await installFromBytesPublic(bytes, entry.sha256, 'market');
};

// 本地导入：选 zip → 读 → 装（共享安装管线；浏览器调试环境由安装器给出人话报错）
export const importLocalPlugin = async (path: string): Promise<void> => {
  const bytes = await invoke<number[]>('read_dialog_file', { path });
  await installFromBytesPublic(new Uint8Array(bytes), undefined, 'local');
};
