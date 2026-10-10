// 插件包安装器：sha256 校验（纯 TS）→ fflate 解压 → 清单校验 → 逐文件写入
// app_data/plugins/<id>/（走 Rust 文件桥）。市场下载与本地导入共用此入口。
import { unzipSync } from 'fflate';
import { invoke } from '@tauri-apps/api/core';
import { sha256Hex } from './sha256';
import { PLUGIN_API_VERSION, type PluginManifest } from './types';

const ID_RE = /^[a-z0-9][a-z0-9-]{0,63}$/;

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

// 安装包布局：plugin.json 在 zip 根，或所有文件共用唯一顶层目录（如 <id>/plugin.json）
const locateManifest = (files: Record<string, Uint8Array>): { manifest: PluginManifest; prefix: string } => {
  const fileNames = Object.keys(files).filter((n) => !n.endsWith('/'));
  if (files['plugin.json']) return { manifest: parseManifest(new TextDecoder().decode(files['plugin.json'])), prefix: '' };
  const candidates = fileNames.filter((n) => n.endsWith('/plugin.json') && n.split('/').length === 2);
  if (candidates.length === 1) {
    const prefix = candidates[0].slice(0, candidates[0].indexOf('/') + 1);
    return { manifest: parseManifest(new TextDecoder().decode(files[candidates[0]])), prefix };
  }
  throw new Error('安装包中找不到 plugin.json（需在 zip 根或唯一顶层目录下）');
};

// 安装：校验哈希 → 解压 → 校验清单 → 写入 app_data/plugins/<id>/。已装同 id 直接覆盖。
export const installFromBytes = async (bytes: Uint8Array, expectedSha256?: string): Promise<PluginManifest> => {
  requireTauri('安装插件');
  const hash = await sha256Hex(bytes);
  if (expectedSha256 && hash.toLowerCase() !== expectedSha256.toLowerCase()) {
    throw new Error('SHA-256 校验失败：安装包与来源描述不一致，已取消安装');
  }
  let files: Record<string, Uint8Array>;
  try {
    files = unzipSync(bytes);
  } catch {
    throw new Error('无法解压安装包：不是有效的 zip 文件');
  }
  const { manifest, prefix } = locateManifest(files);
  const names = Object.keys(files).filter((n) => !n.endsWith('/') && n.startsWith(prefix));
  if (names.length === 0) throw new Error('安装包为空');
  if (prefix && names.length !== Object.keys(files).filter((n) => !n.endsWith('/')).length) {
    throw new Error('安装包布局不合法：存在多个顶层目录');
  }
  for (const name of names) {
    const rel = `${manifest.id}/${name.slice(prefix.length)}`;
    await invoke('plugin_write_file', { relPath: rel, bytes: Array.from(files[name]) });
  }
  return manifest;
};
