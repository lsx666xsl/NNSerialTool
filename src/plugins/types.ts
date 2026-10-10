// ==================================================================
// 插件系统类型契约（权威定义）。
// 插件仓库（NNSerialTool-Plugins）的 README、schema.json 与此同源，
// 修改任一字段必须同步三处并在工作日志记录 apiVersion 变更。
// ==================================================================
import type { Component } from 'vue';

// 插件 API 版本：应用升级时只追加保持兼容；破坏性变更才递增。
export const PLUGIN_API_VERSION = 1;

export type ThemeName = 'light' | 'dark';

// 插件能力声明：未声明的能力调用时直接抛错（安装界面会展示该清单）。
export type PluginPermission = 'send' | 'storage';

// plugin.json 清单（zip 包根目录或唯一顶层目录下）
export interface PluginManifest {
  id: string; // 全局唯一：小写字母/数字/连字符，1~64 字符
  name: string;
  version: string; // 语义化版本
  apiVersion: number; // 必须等于应用支持的 PLUGIN_API_VERSION
  type: 'view' | 'protocol'; // 视图插件 / 协议插件
  entry: string; // 入口 js（相对包根，默认 main.js）
  author?: string;
  description?: string;
  permissions?: PluginPermission[];
  minAppVersion?: string;
}

// 会话快照（数据源选择用）
export interface SessionSnapshot {
  id: string;
  name: string;
  type: 'serial' | 'tcp_client' | 'tcp_server' | 'udp';
  status: 'connected' | 'closed';
}

// 原始字节事件：会话 RX 到达时广播给插件
export interface RawDataEvent {
  sessionId: string;
  sessionName: string;
  bytes: Uint8Array;
  t: number; // performance.now() 到达时间戳（ms），波形 X 轴基准
}

// 解析器吐出的帧：channels 与协议定义的通道一一对应
export interface WaveFrame {
  t: number;
  channels: number[];
  seq?: number; // NN-Wave 帧序号（丢帧检测用），其他协议可省略
}

// 协议插件注册的解析器实例：自持状态机，字节流按到达顺序连续喂入
export interface ParserInstance {
  feed(bytes: Uint8Array, t: number): WaveFrame[];
  reset(): void;
}

export interface ProtocolDef {
  id: string;
  name: string;
  description?: string;
  createParser(): ParserInstance;
}

// 视图插件注册项：出现在功能切换列表（单屏/分屏/总览/转发之后）
export interface ViewDef {
  id: string; // viewMode 键；第三方插件自动加 "plugin-<插件id>" 前缀防冲突
  name: string; // 按钮文案
  tip: string; // 悬停说明
  blocks?: number; // 迷你缩略图方块数（1~4，缺省 2）
  component: Component; // Vue 组件；普通 JS 插件用 host.mount 契约包装（见开发指南）
}

// 普通 JS 插件（无 Vue 运行时）的视图挂载契约：
// mount 在视图显示时被调用，返回清理函数或提供 unmount；插件在 el 内自建 DOM。
export interface PluginViewHandle {
  mount(el: HTMLElement, ctx: PluginContext): void | (() => void);
  unmount?(el: HTMLElement): void;
}

// 主题取色：跟随应用深浅色，禁止插件写死颜色
export interface ThemeColors {
  bg: string; // 画布/内容底色
  panel: string; // 面板底色
  border: string;
  text: string;
  textDim: string; // 次要文字
  grid: string; // 网格线
  accent: string; // 强调色（蓝）
  palette: string[]; // 通道配色（≥8 色，两套主题通用）
}

// 应用注入给插件的能力面（白名单，拿不到原始 Tauri IPC）
export interface PluginContext {
  readonly pluginId: string;
  theme(): ThemeName;
  themeColors(): ThemeColors;
  onThemeChange(cb: (t: ThemeName) => void): () => void;
  listSessions(): SessionSnapshot[];
  onSessionsChange(cb: (list: SessionSnapshot[]) => void): () => void;
  onRawData(cb: (e: RawDataEvent) => void): () => void;
  send(sessionId: string, bytes: Uint8Array | number[]): Promise<void>; // 需权限 'send'
  notify(text: string): void;
  exportTextFiles(title: string, files: Array<{ name: string; text: string }>, folderName?: string): Promise<string>; // 弹目录选择后在 folderName 子目录中覆盖写出，返回目录（取消返回 ''）
  storage: {
    get<T>(key: string, fallback: T): T; // 需权限 'storage'
    set(key: string, value: unknown): void;
  };
  registerView(def: ViewDef): void;
  registerProtocol(def: ProtocolDef): void;
  log(...args: unknown[]): void;
}

// appStore 在 initApp 时注入的服务桥（避免 appStore ↔ host 循环依赖）
// 插件实例契约：main.js 默认导出 activate（ES module）
export interface PluginInstance {
  manifest: PluginManifest;
  activate(ctx: PluginContext): void | Promise<void>;
  deactivate?(): void;
}

export interface AppBridge {
  sessions(): SessionSnapshot[];
  onSessions(cb: (list: SessionSnapshot[]) => void): () => void;
  sendRaw(sessionId: string, bytes: number[]): Promise<void>;
  notify(text: string): void;
}

// appStore 在 initApp 时注入的服务桥（避免 appStore ↔ host 循环依赖）
export interface AppBridge {
  sessions(): SessionSnapshot[];
  sendRaw(sessionId: string, bytes: number[]): Promise<void>;
  notify(text: string): void;
}
