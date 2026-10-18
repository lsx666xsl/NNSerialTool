// 插件包安装器：sha256 校验（纯 TS）→ fflate 解压 → 条目/清单全量校验 → 单次原子安装。
// 市场下载与本地导入共用此入口；文件落盘由 Rust 命令 plugin_install 一次性完成
// （后端先写临时目录再整体替换，任一环失败自动回滚），前端不再逐个文件写入。
import { unzipSync } from 'fflate';
import { invoke } from '@tauri-apps/api/core';
import { sha256Hex } from './sha256';
import { PLUGIN_API_VERSION, type PluginManifest } from './types';
import { compareVersion } from '../utils/update';

const ID_RE = /^[a-z0-9][a-z0-9-]{0,63}$/;
const SHA256_RE = /^[a-f0-9]{64}$/i;

// 安装包资源上限：与后端校验口径保持一致；超限直接拒绝（避免解压炸弹拖垮内存）
const MAX_FILES = 500; // 条目数上限
const MAX_TOTAL_BYTES = 32 * 1024 * 1024; // 解压后总字节上限 32MB
const MAX_FILE_BYTES = 16 * 1024 * 1024; // 单文件上限 16MB
const MAX_SEGMENTS = 8; // relPath 段数上限（与 Rust safe_rel 一致）

// 浏览器调试环境（如 dev 页面被普通浏览器打开）没有 Tauri IPC，插件写入/读取必须在桌面窗口进行
export const requireTauri = (action: string) => {
  if (!(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__) {
    throw new Error(`当前是浏览器调试环境（无 Tauri 后端），无法${action}——请在桌面应用窗口中操作`);
  }
};

// 清单校验：字段齐全 + 类型正确；返回带默认值的清单，不合法抛错（错误信息面向插件作者）
export const validateManifest = (raw: unknown): PluginManifest => {
  const bad = (why: string) => new Error(`plugin.json 无效：${why}`);
  if (typeof raw !== 'object' || raw === null) throw bad('不是 JSON 对象');
  const m = raw as Record<string, unknown>;
  const str = (v: unknown) => typeof v === 'string' && v.trim().length > 0;
  if (!str(m.id) || !ID_RE.test(String(m.id))) throw bad('id 必须为 1~64 位小写字母/数字/连字符，且以字母或数字开头');
  if (!str(m.name)) throw bad('缺少 name');
  if (!str(m.version)) throw bad('缺少 version');
  if (m.apiVersion !== PLUGIN_API_VERSION)
    throw bad(`apiVersion 必须为 ${PLUGIN_API_VERSION}（当前应用插件 API 版本），收到 ${JSON.stringify(m.apiVersion)}`);
  if (m.type !== 'view' && m.type !== 'protocol') throw bad("type 必须为 'view' 或 'protocol'");
  const entry = str(m.entry) ? String(m.entry).trim() : 'main.js';
  if (!/^[A-Za-z0-9._-]{1,64}$/.test(entry)) throw bad('entry 只允许文件名（字母/数字/._-），如 main.js');
  if (m.permissions !== undefined) {
    const perms = m.permissions;
    if (!Array.isArray(perms) || perms.some((p) => p !== 'send' && p !== 'storage'))
      throw bad("permissions 只允许 'send' / 'storage'");
  }
  return {
    id: String(m.id),
    name: String(m.name),
    version: String(m.version),
    apiVersion: PLUGIN_API_VERSION,
    type: m.type,
    entry,
    author: m.author === undefined ? undefined : String(m.author),
    description: m.description === undefined ? undefined : String(m.description),
    permissions: Array.isArray(m.permissions) ? (m.permissions as PluginManifest['permissions']) : [],
    minAppVersion: m.minAppVersion === undefined ? undefined : String(m.minAppVersion),
  };
};

export const parseManifest = (text: string): PluginManifest => {
  try {
    return validateManifest(JSON.parse(text));
  } catch (e) {
    if (e instanceof SyntaxError) throw new Error(`plugin.json 不是合法 JSON：${e.message}`);
    throw e;
  }
};

// ---------- 包内路径安全校验（与 Rust safe_rel 完全同规则） ----------
// 反斜杠统一成正斜杠；禁止绝对路径/UNC、盘符、"."/".."、空段、非法字符、
// 以点结尾的段（Windows 会裁掉结尾点，可能造成同名覆盖），段数 ≤ 8。
const normalizeRel = (raw: string) => raw.replace(/\\/g, '/');

const isSafeRel = (relPath: string): boolean => {
  const normalized = normalizeRel(relPath);
  if (!normalized || normalized.startsWith('/')) return false; // 空 / 绝对路径 / UNC
  const segs = normalized.split('/');
  if (segs.length > MAX_SEGMENTS) return false;
  // Windows 保留设备名在任意目录下都不能作为文件名（与 Rust safe_rel 对齐）
  const RESERVED = /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(\.|$)/i;
  return segs.every(
    (seg) =>
      seg.length > 0 &&
      seg !== '.' &&
      seg !== '..' &&
      !seg.endsWith('.') &&
      !RESERVED.test(seg) &&
      /^[A-Za-z0-9._-]+$/.test(seg)
  );
};

// 解压结果键名归一化：部分 Windows 打包工具会把 zip 内路径写成反斜杠，
// 统一成正斜杠后再定位清单/校验路径，避免"看起来存在却找不到 plugin.json"。
const normalizeFiles = (raw: Record<string, Uint8Array>): Record<string, Uint8Array> => {
  const files: Record<string, Uint8Array> = {};
  for (const [name, content] of Object.entries(raw)) {
    const relPath = normalizeRel(name);
    if (!relPath || relPath.endsWith('/')) continue; // 跳过目录条目
    // 归一化后重复（如 a/b.js 与 a\b.js 同时存在）：拒绝安装，避免覆盖顺序不可预期
    if (files[relPath]) throw new Error(`安装包内存在重复文件路径：${relPath}`);
    files[relPath] = content;
  }
  return files;
};

// 安装包布局：plugin.json 在 zip 根，或所有文件共用唯一顶层目录（如 <id>/plugin.json）
const locateManifest = (files: Record<string, Uint8Array>): { manifest: PluginManifest; prefix: string } => {
  if (files['plugin.json']) return { manifest: parseManifest(new TextDecoder().decode(files['plugin.json'])), prefix: '' };
  const candidates = Object.keys(files).filter((n) => n.endsWith('/plugin.json') && n.split('/').length === 2);
  if (candidates.length === 1) {
    const prefix = candidates[0].slice(0, candidates[0].indexOf('/') + 1);
    return { manifest: parseManifest(new TextDecoder().decode(files[candidates[0]])), prefix };
  }
  throw new Error('安装包中找不到 plugin.json（需在 zip 根或唯一顶层目录下）');
};

export interface InstallOptions {
  // 市场安装强制要求 sha256 校验值（索引不完整时拒绝）
  requireSha256?: boolean;
  // 索引声明的元信息：清单必须与之一致，防止索引与实际安装包被替换
  expect?: { id?: string; version?: string; apiVersion?: number; minAppVersion?: string };
}

// 安装：校验哈希 → 解压 → 条目全量校验 → 单次原子安装到 app_data/plugins/<id>/。
// 已装同 id 直接覆盖（覆盖前由 host 先卸载旧实例）。
export const installFromBytes = async (
  bytes: Uint8Array,
  expectedSha256?: string,
  options: InstallOptions = {}
): Promise<PluginManifest> => {
  requireTauri('安装插件');
  // 市场安装：索引必须带合法 sha256，缺失即视为索引不完整，直接拒绝
  if (options.requireSha256 && (!expectedSha256 || !SHA256_RE.test(expectedSha256))) {
    throw new Error('插件索引缺少合法的 sha256 校验值，已拒绝安装（索引不完整）');
  }
  if (expectedSha256 && !SHA256_RE.test(expectedSha256)) {
    throw new Error('sha256 校验值格式非法（需 64 位十六进制），已拒绝安装');
  }
  const hash = await sha256Hex(bytes);
  if (expectedSha256 && hash.toLowerCase() !== expectedSha256.toLowerCase()) {
    throw new Error('SHA-256 校验失败：安装包与来源描述不一致，已取消安装');
  }
  let unzipped: Record<string, Uint8Array>;
  try {
    unzipped = unzipSync(bytes);
  } catch {
    throw new Error('无法解压安装包：不是有效的 zip 文件');
  }
  const files = normalizeFiles(unzipped);
  const { manifest, prefix } = locateManifest(files);

  // 市场条目核对：id / 版本 / apiVersion / minAppVersion 与索引声明不一致即拒绝
  const expect = options.expect;
  if (expect) {
    if (expect.id && expect.id !== manifest.id) {
      throw new Error(`安装包 id（${manifest.id}）与市场索引（${expect.id}）不一致，已取消安装`);
    }
    if (expect.version && compareVersion(expect.version, manifest.version) !== 0) {
      throw new Error(`安装包版本 v${manifest.version} 与市场索引 v${expect.version} 不一致，已取消安装`);
    }
    if (expect.apiVersion !== undefined && expect.apiVersion !== manifest.apiVersion) {
      throw new Error('安装包 apiVersion 与市场索引声明不一致，已取消安装');
    }
    if (expect.minAppVersion !== undefined && expect.minAppVersion !== manifest.minAppVersion) {
      throw new Error('安装包 minAppVersion 与市场索引声明不一致，已取消安装');
    }
  }

  // 全量条目校验与汇总：任何一条路径越界/体积超限都整包拒绝（不做部分写入）
  const entries: Array<{ relPath: string; content: Uint8Array }> = [];
  let totalBytes = 0;
  for (const [name, content] of Object.entries(files)) {
    if (prefix && !name.startsWith(prefix)) throw new Error('安装包布局不合法：存在多个顶层目录');
    const relPath = prefix ? name.slice(prefix.length) : name;
    if (!isSafeRel(relPath)) throw new Error(`安装包内文件路径不合法：${relPath}`);
    if (content.length > MAX_FILE_BYTES) {
      throw new Error(`单个插件文件超过上限（${Math.round(MAX_FILE_BYTES / 1024 / 1024)}MB）：${relPath}`);
    }
    totalBytes += content.length;
    if (totalBytes > MAX_TOTAL_BYTES) {
      throw new Error(`插件解压后总大小超过上限（${Math.round(MAX_TOTAL_BYTES / 1024 / 1024)}MB），已取消安装`);
    }
    entries.push({ relPath, content });
  }
  if (entries.length === 0) throw new Error('安装包为空');
  if (entries.length > MAX_FILES) throw new Error(`安装包文件数超过上限（${MAX_FILES} 个），已取消安装`);

  // 单次 IPC 原子安装：relPath 相对插件目录（后端按 id 拼接插件根目录），
  // 不以 id 前缀；任一文件失败由后端整体回滚，前端无需（也无法）自行清理。
  await invoke('plugin_install', {
    id: manifest.id,
    files: entries.map((entry) => ({ relPath: entry.relPath, bytes: Array.from(entry.content) })),
  });
  return manifest;
};
