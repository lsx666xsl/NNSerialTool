// ==================================================================
// 插件宿主：加载/卸载第三方插件、内置插件激活、能力上下文工厂、
// 原始数据分发。第三方 JS 通过 Blob URL 动态 import，只拿得到
// PluginContext 白名单能力，接触不到原始 Tauri IPC。
// ==================================================================
import { defineComponent, h, onMounted, onUnmounted, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { unregisterBySource, registerProtocol, registerView } from './registry';
import { installFromBytes, parseManifest } from './installer';
import { crumb } from '../debug/stall-watchdog';
import { readStorage, writeStorage } from '../utils/storage';
import { appVersion } from '../stores/updateStore';
import { compareVersion } from '../utils/update';
import type {
  AppBridge,
  PluginContext,
  PluginInstance,
  PluginManifest,
  PluginViewHandle,
  RawDataEvent,
  ThemeColors,
  ThemeName,
  ViewDef,
} from './types';

const DISABLED_KEY = 'st-plugins-disabled';

// ---------- 主题（appStore 通过 setTheme 注入，插件经 ctx 读取） ----------
let currentTheme: ThemeName = 'light';
const themeListeners = new Set<(t: ThemeName) => void>();

export const setTheme = (t: ThemeName) => {
  if (currentTheme === t) return;
  currentTheme = t;
  themeListeners.forEach((cb) => cb(t));
};

// ---------- 应用服务桥（appStore.initApp 第一行装配） ----------
let bridge: AppBridge | null = null;

export const setAppBridge = (b: AppBridge) => {
  bridge = b;
};

// ---------- 运行状态 ----------
const loaded = new Map<string, { instance: PluginInstance; url?: string }>();
const loadingIds = new Set<string>();
const rawListeners = new Map<string, Set<(e: RawDataEvent) => void>>();
const disabled = new Set(readStorage<string[]>(DISABLED_KEY, []));

// 已安装清单（PluginManager 面板数据源；内置项常驻）
export interface InstalledEntry {
  id: string;
  name: string;
  version: string;
  type: string;
  author?: string;
  description?: string;
  enabled: boolean;
  source: 'builtin' | 'local' | 'market';
  error?: string;
}
export const installedList = ref<InstalledEntry[]>([]);

// 每插件加载失败原因（已安装列表条目内展示；此前是全局字符串且无 UI 消费，属死通道）
const loadErrors = new Map<string, string>();

// ---------- 主题取色表（与应用样式同源的色板，Canvas 类插件用） ----------
const THEME_COLORS: Record<ThemeName, ThemeColors> = {
  light: {
    bg: '#f8f9fb',
    panel: '#ffffff',
    border: 'rgba(23, 26, 33, 0.12)',
    text: '#23262b',
    textDim: '#8a9099',
    grid: 'rgba(23, 26, 33, 0.08)',
    accent: '#3b6fd4',
    palette: ['#2b6cb0', '#c74541', '#2e8b45', '#b7791f', '#7c5cbf', '#0e8a8a', '#d16ba5', '#5a7d2a'],
  },
  dark: {
    bg: '#1e1f22',
    panel: '#2b2d30',
    border: 'rgba(255, 255, 255, 0.12)',
    text: '#d6d9de',
    textDim: '#7c828c',
    grid: 'rgba(255, 255, 255, 0.08)',
    accent: '#6ca7e8',
    palette: ['#6ca7e8', '#e0837f', '#6bc97e', '#d9a441', '#a78bda', '#4fc3c3', '#e391c4', '#9dc26b'],
  },
};

// ---------- 能力上下文工厂 ----------
// 导出给内置插件复用（波形等内置功能与第三方走同一套能力面）
export const createInternalContext = (manifest: PluginManifest): PluginContext => {
  const pid = manifest.id;
  const perms = manifest.permissions ?? [];
  const storageKey = (key: string) => `st-plugin-${pid}:${key}`;
  const ctx: PluginContext = {
    pluginId: pid,
    theme: () => currentTheme,
    themeColors: () => THEME_COLORS[currentTheme],
    onThemeChange(cb) {
      themeListeners.add(cb);
      return () => themeListeners.delete(cb);
    },
    listSessions: () => bridge?.sessions() ?? [],
    onSessionsChange(cb) {
      return bridge ? bridge.onSessions(cb) : () => {};
    },
    onRawData(cb) {
      const set = rawListeners.get(pid) ?? new Set();
      set.add(cb);
      rawListeners.set(pid, set);
      return () => set.delete(cb);
    },
    async send(sessionId, bytes) {
      if (!perms.includes('send')) throw new Error(`插件 ${pid} 未声明 'send' 权限`);
      if (!bridge) throw new Error('应用服务未就绪');
      await bridge.sendRaw(sessionId, bytes instanceof Uint8Array ? Array.from(bytes) : bytes);
    },
    notify: (text) => bridge?.notify(text),
    async exportTextFiles(title, files, folderName) {
      // 用户经原生目录对话框知情选择后在子目录中覆盖写出；取消返回空串
      const { open } = await import('@tauri-apps/plugin-dialog');
      const dir = await open({ directory: true, multiple: false, title });
      if (typeof dir !== 'string' || !dir) return '';
      const target = folderName ? dir + '/' + folderName : dir;
      for (const f of files) {
        await invoke('write_text_file', { dir: target, filename: f.name, text: f.text });
      }
      return target;
    },
    storage: {
      get(key, fallback) {
        if (!perms.includes('storage')) throw new Error(`插件 ${pid} 未声明 'storage' 权限`);
        return readStorage(storageKey(key), fallback);
      },
      set(key, value) {
        if (!perms.includes('storage')) throw new Error(`插件 ${pid} 未声明 'storage' 权限`);
        writeStorage(storageKey(key), value);
      },
    },
    registerView(def: ViewDef | (Omit<ViewDef, 'component'> & { component: PluginViewHandle })) {
      const id = def.id.startsWith('builtin-') ? def.id : `plugin-${pid}-${def.id}`;
      const isHandle = def.component && typeof (def.component as PluginViewHandle).mount === 'function';
      // 普通 JS 插件的 DOM handle 包装成 Vue 组件：挂载根 div + 生命周期转接
      const component = isHandle
        ? defineComponent({
            setup() {
              const el = ref<HTMLElement | null>(null);
              let cleanup: (() => void) | undefined;
              let mountedEl: HTMLElement | null = null;
              onMounted(() => {
                if (!el.value) return;
                mountedEl = el.value;
                const r = (def.component as PluginViewHandle).mount(mountedEl, ctx);
                if (typeof r === 'function') cleanup = r;
              });
              onUnmounted(() => {
                cleanup?.();
                try {
                  // 卸载时 Vue 已把模板引用置空——用挂载时捕获的元素，避免 null.innerHTML
                  (def.component as PluginViewHandle).unmount?.(mountedEl as HTMLElement);
                } catch (e) {
                  console.warn('[plugin:' + pid + '] unmount 异常', e);
                }
                mountedEl = null;
              });
              return () =>
                h('div', {
                  ref: el,
                  class: 'plugin-view-root',
                  // 与 .content-card 同款填充链：占满 main-area 剩余高度，插件视图才能纵向撑满
                  style: 'flex:1;min-height:0;display:flex;flex-direction:column;min-width:0;',
                });
            },
          })
        : def.component;
      registerView({ ...(def as ViewDef), id, component, source: pid });
      refreshInstalled(pid);
    },
    registerProtocol(def) {
      registerProtocol({ ...def, id: def.id.startsWith('builtin-') ? def.id : `${pid}/${def.id}`, source: pid });
    },
    log: (...args) => console.log(`[plugin:${pid}]`, ...args),
  };
  return ctx;
};

// ---------- 数据分发（appStore RX tap 调用） ----------
let dispatchCount = 0;
export const dispatchRawData = (sessionId: string, sessionName: string, bytes: number[] | Uint8Array) => {
  if (rawListeners.size === 0) return;
  dispatchCount++;
  if (dispatchCount % 10 === 1) crumb('dispatch#' + dispatchCount + ' ' + sessionName + ' ' + (bytes.length ?? 0) + 'B listeners=' + rawListeners.size);
  const event: RawDataEvent = {
    sessionId,
    sessionName,
    bytes: bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes),
    t: performance.now(),
  };
  for (const set of rawListeners.values()) set.forEach((cb) => cb(event));
};

// ---------- 加载 / 卸载 ----------
const blobImport = async (code: string): Promise<{ activate?: unknown; deactivate?: unknown }> => {
  const url = URL.createObjectURL(new Blob([code], { type: 'text/javascript' }));
  try {
    return await import(/* @vite-ignore */ url);
  } finally {
    URL.revokeObjectURL(url);
  }
};

const unloadPlugin = (id: string) => {
  const entry = loaded.get(id);
  if (!entry) return;
  try {
    entry.instance.deactivate?.();
  } catch (e) {
    console.warn(`[plugin:${id}] deactivate 异常`, e);
  }
  unregisterBySource(id);
  rawListeners.delete(id);
  loaded.delete(id);
};

const loadPlugin = async (id: string): Promise<void> => {
  // 防重入：同一插件禁止并发加载——快速连点启用/停用会让两次 loadPlugin 叠加，
  // 产生双实例双数据订阅（僵尸监听翻倍），是启用路径卡顿/卡死的候选根因
  if (loaded.has(id) || loadingIds.has(id)) return;
  loadingIds.add(id);
  crumb('loadPlugin start ' + id);
  try {
    const manifest = parseManifest(await invoke<string>('plugin_read_text', { relPath: `${id}/plugin.json` }));
    manifests.set(id, manifest);
    if (manifest.apiVersion > 1)
      throw new Error(`插件 apiVersion=${manifest.apiVersion} 高于当前应用支持的版本，请升级应用`);
    if (manifest.minAppVersion && appVersion.value && compareVersion(manifest.minAppVersion, appVersion.value) > 0) {
      throw new Error(`插件要求应用 ≥ v${manifest.minAppVersion}`);
    }
    const code = await invoke<string>('plugin_read_text', { relPath: `${id}/${manifest.entry}` });
    const mod = (await blobImport(code)) as Record<string, unknown>;
    // 入口契约三形态兼容：export default function activate（默认导出即函数）/
    // export default { activate, deactivate }（对象形态）/ export function activate（具名导出）。
    // 插件仓库的示例与文档（hello-view、wave）均用第一种，此前只认对象形态导致全部误报缺导出。
    const dflt = mod.default as unknown;
    const activate =
      typeof dflt === 'function'
        ? (dflt as PluginInstance['activate'])
        : dflt && typeof (dflt as { activate?: unknown }).activate === 'function'
          ? (dflt as { activate: PluginInstance['activate'] }).activate
          : typeof mod.activate === 'function'
            ? (mod.activate as PluginInstance['activate'])
            : undefined;
    if (typeof activate !== 'function') {
      throw new Error(
        '入口文件缺少 activate 导出（支持 export default activate / export default { activate } / export function activate）'
      );
    }
    const deactivate =
      dflt && typeof (dflt as { deactivate?: unknown }).deactivate === 'function'
        ? (dflt as { deactivate: PluginInstance['deactivate'] }).deactivate
        : typeof mod.deactivate === 'function'
          ? (mod.deactivate as PluginInstance['deactivate'])
          : undefined;
    const instance: PluginInstance = {
      manifest,
      activate,
      deactivate,
    };
    crumb('activate done ' + id);
    await instance.activate(createInternalContext(manifest));
    crumb('registered ' + id);
    // 加载期间被卸载/停用（竞态）：注册即撤销，避免僵尸监听与僵尸视图
    if (!sourceOf.has(id) || disabled.has(id)) {
      unregisterBySource(id);
      rawListeners.delete(id);
      return;
    }
    loaded.set(id, { instance });
    crumb('loadPlugin done ' + id);
    refreshInstalled(id);
  } finally {
    loadingIds.delete(id);
  }
};

// ---------- 安装清单维护 ----------
const sourceOf = new Map<string, 'local' | 'market'>();
// 清单独立缓存：与加载状态解耦——停用/加载失败的插件也要在列表里显示完整详情
const manifests = new Map<string, PluginManifest>();

const refreshInstalled = (focusId?: string) => {
  const entries: InstalledEntry[] = [];
  // 注意：sourceOf 是 Map，必须用 Map 的 keys() 遍历——
  // Object.keys(Map) 恒返回空数组（Map 键不是对象自有可枚举属性），曾致列表永远为空
  for (const id of sourceOf.keys()) {
    const inList = manifests.get(id);
    entries.push({
      id,
      name: inList?.name ?? id,
      version: inList?.version ?? '?',
      type: inList?.type ?? '?',
      author: inList?.author,
      description: inList?.description,
      enabled: !disabled.has(id),
      source: sourceOf.get(id) ?? 'local',
      error: loadErrors.get(id),
    });
  }
  installedList.value = entries;
  void focusId;
};

// ---------- 公开操作（PluginManager 调用） ----------
export const isEnabled = (id: string) => !disabled.has(id);

export const setEnabled = (id: string, on: boolean) => {
  crumb('setEnabled ' + id + ' -> ' + (on ? 'on' : 'off'));
  if (on) disabled.delete(id);
  else disabled.add(id);
  writeStorage(DISABLED_KEY, [...disabled]);
  if (on) {
    // 停用曾卸载运行时：重新启用必须立即恢复加载，功能切换即时回归、详情即时补全
    if (!loaded.has(id)) {
      loadPlugin(id)
        .then(() => {
          loadErrors.delete(id);
          refreshInstalled(id);
        })
        .catch((e) => {
          loadErrors.set(id, e instanceof Error ? e.message : String(e));
          refreshInstalled(id);
        });
    }
  } else {
    unloadPlugin(id);
  }
  refreshInstalled(id);
};

// 卸载即清除：连带清掉插件私有 storage（localStorage 键 st-plugin-<id>:*）
const purgeStorage = (id: string) => {
  const prefix = `st-plugin-${id}:`;
  const stale: string[] = [];
  for (let i = 0; i < localStorage.length; i++) {
    const key = localStorage.key(i);
    if (key && key.startsWith(prefix)) stale.push(key);
  }
  stale.forEach((key) => localStorage.removeItem(key));
};

export const uninstallPlugin = async (id: string) => {
  unloadPlugin(id);
  sourceOf.delete(id);
  purgeStorage(id);
  await invoke('plugin_remove_dir', { id }).catch(() => {});
  refreshInstalled(id);
};

export const installFromBytesPublic = async (bytes: Uint8Array, expectedSha256?: string, source: 'local' | 'market' = 'market'): Promise<PluginManifest> => {
  const manifest = await installFromBytes(bytes, expectedSha256);
  sourceOf.set(manifest.id, source);
  disabled.delete(manifest.id);
  writeStorage(DISABLED_KEY, [...disabled]);
  // 覆盖安装/更新：必须先卸载旧实例——loadPlugin 的防重入守卫看到旧实例还在会直接返回，
  // 新版本将永远加载不上（曾致市场"更新成功"后运行版本不变）
  unloadPlugin(manifest.id);
  manifests.delete(manifest.id);
  await loadPlugin(manifest.id);
  refreshInstalled(manifest.id);
  return manifest;
};

// ---------- 启动：加载所有已启用插件 ----------
export const initPlugins = async () => {
  let ids: string[] = [];
  try {
    ids = await invoke<string[]>('plugin_list_ids');
  } catch {
    return; // 浏览器调试环境无 Tauri API，静默跳过
  }
  // 先登记全部来源（含停用的）：停用插件也必须出现在已安装列表，否则无法从界面重新启用
  ids.forEach((id) => {
    if (!sourceOf.has(id)) sourceOf.set(id, 'local');
  });
  refreshInstalled();
  for (const id of ids) {
    if (disabled.has(id)) continue;
    try {
      await loadPlugin(id);
    } catch (e) {
      loadErrors.set(id, e instanceof Error ? e.message : String(e));
      refreshInstalled();
    }
  }
};

// 已安装列表全量对账（管理面板打开时与"刷新"按钮共用）：
// ① 补上磁盘上新增的插件目录 ② 清掉磁盘已不存在的条目（外部卸载/手动删除）
// ③ 对未加载且未停用的条目重试加载（给启动时失败过的插件恢复机会）。
// 返回摘要文案供 toast 展示；扫描失败时返回错误说明（面板内可见，便于诊断）。
export const syncInstalledFromDisk = async (): Promise<string> => {
  let ids: string[] = [];
  try {
    ids = await invoke<string[]>('plugin_list_ids');
  } catch (e) {
    return `扫描已装插件失败：${e instanceof Error ? e.message : e}`;
  }
  for (const id of [...sourceOf.keys()]) {
    if (!ids.includes(id)) {
      unloadPlugin(id);
      sourceOf.delete(id);
      loadErrors.delete(id);
    }
  }
  for (const id of ids) {
    if (!sourceOf.has(id)) sourceOf.set(id, 'local');
    if (!manifests.has(id)) {
      // 停用/从未加载的插件也读清单：列表详情（名称/版本/类型/描述）不因停用而丢失
      try {
        manifests.set(id, parseManifest(await invoke<string>('plugin_read_text', { relPath: `${id}/plugin.json` })));
      } catch {
        /* 清单不可读：条目退回 id 展示 */
      }
    }
    if (!loaded.has(id) && !disabled.has(id)) {
      try {
        await loadPlugin(id);
        loadErrors.delete(id);
      } catch (e) {
        loadErrors.set(id, e instanceof Error ? e.message : String(e));
      }
    }
  }
  refreshInstalled();
  return ids.length > 0 ? `已安装 ${ids.length} 个插件（${ids.join('、')}）` : '暂无已安装插件';
};
