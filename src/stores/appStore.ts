import { computed, reactive, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  ConnectionSession,
  ForwardRule,
  GlobalMessage,
  MessageDirection,
  NetConfig,
  QuickCommand,
  SendSettings,
  SerialConfig,
  SerialPortInfo,
  SessionMessage,
  ThemeMode,
} from '../types';
import { isBoolean, isFiniteNumber, isString, readStorage, readStorageChecked, writeStorage } from '../utils/storage';
import { bytesToHex, decodeBytes, hexToBytes, nowText, todayText } from '../utils/format';
import { netSessionName } from '../utils/session';
import { dispatchRawData, setAppBridge, setTheme } from '../plugins/host';
import { crumb } from '../debug/stall-watchdog';

// ==================================================================
// 单例应用状态仓：整个应用只有一份，各组件直接 import 使用。
// initApp()/disposeApp() 负责事件监听与定时器的装配和清理，由 App.vue 调用。
// ==================================================================

// ---------- 下拉选项（来自 Rust 后端，多个会话共享同一份） ----------
export const ports = ref<SerialPortInfo[]>([]);
export const baudRates = ref<number[]>([]);
export const dataBits = ref<number[]>([]);
export const parityBits = ref<string[]>([]);
export const stopBits = ref<number[]>([]);
// 本机网卡 IPv4 列表，地址输入框旁的下拉快速选择用
export const localIps = ref<string[]>([]);

// ---------- 新建会话表单 ----------
export const newSessionType = ref<ConnectionSession['type']>('serial');
export const newSessionConfig = ref<SerialConfig>({
  port: '',
  baudRate: 9600,
  dataBits: 8,
  parityBits: 'None',
  stopBits: 1,
});
export const newNetConfig = ref<NetConfig>({ host: '127.0.0.1', port: 9000, localPort: 9000, localHost: '' });

// ---------- 会话与视图 ----------
export const sessions = ref<ConnectionSession[]>([]);
export const globalMessages = ref<GlobalMessage[]>([]);
export const activeSessionId = ref<string>('');
export const selectedSessionIds = ref<string[]>([]);
export const viewMode = ref<import('../types').ViewMode>('detail');
watch(viewMode, (v) => crumb('viewMode -> ' + v));

// ---------- 主题：浅色 / 深色 / 系统 ----------
// themeMode 是用户的选择，appliedTheme 是最终落到 DOM 上的类名；
// system 模式下监听系统 prefers-color-scheme，跟随系统实时切换。
const isThemeMode = (value: unknown): value is ThemeMode =>
  value === 'light' || value === 'dark' || value === 'system';
// 存储读取加类型守卫：旧版本/手工改坏的值直接回退默认，避免非法主题类名落到 DOM
export const themeMode = ref<ThemeMode>(readStorageChecked<ThemeMode>('st-theme', 'system', isThemeMode));
// 模块加载时立即同步一次系统深浅状态，避免深色系统用户启动时闪一帧浅色
const systemDarkQuery = window.matchMedia('(prefers-color-scheme: dark)');
const systemDark = ref(systemDarkQuery.matches);
export const appliedTheme = computed<import('../types').AppliedTheme>(() =>
  themeMode.value === 'system' ? (systemDark.value ? 'dark' : 'light') : themeMode.value
);
watch(themeMode, (mode) => writeStorage('st-theme', mode));
// 主题同步到插件宿主：插件画布取色跟随应用深浅色（不做插件独立主题）
// immediate：启动即深色时也要同步首值，否则宿主停在默认 light、插件画布白底
watch(appliedTheme, (t) => setTheme(t), { immediate: true });

// ---------- 显示与日志偏好 ----------
// 消息标签总开关（时间戳 + RX/TX 方向字符）：纯显示层，不影响数据接收、也不改变方向过滤；
// 关闭后立即隐藏所有消息的标签（包括历史消息）。
// 兼容旧版全局时间戳开关 st-timestamp：新 key 尚未写入时沿用旧偏好，避免升级后回弹到“显示”。
const legacyShowTimestamp = readStorageChecked<boolean>('st-timestamp', true, isBoolean);
export const showMessageMeta = ref(readStorageChecked<boolean>('st-show-meta', legacyShowTimestamp, isBoolean));
watch(showMessageMeta, (v) => writeStorage('st-show-meta', v));
// 左侧连接面板折叠状态：App 控制布局，DetailView/SplitView 用它决定是否把 RX/TX 过滤按钮放进工具带
export const sidebarCollapsed = ref(readStorageChecked<boolean>('st-sidebar-collapsed', false, isBoolean));
watch(sidebarCollapsed, (v) => writeStorage('st-sidebar-collapsed', v));
// 自动滚动默认关闭（安全默认：自动发送/高频模式下用户自己控制是否跟随）
export const autoScroll = ref(readStorageChecked<boolean>('st-autoscroll', false, isBoolean));
watch(autoScroll, (v) => writeStorage('st-autoscroll', v));
// 启动时强制关闭自动滚动（安全默认：不跨启动保留，用户需要时手动勾选）
autoScroll.value = false;
// 消息流字号（Ctrl+滚轮可在接收区实时调整，范围 11-22）：非法存储值回退 12 并夹取到合法区间
export const fontSize = ref(
  Math.min(22, Math.max(11, Math.round(readStorageChecked<number>('st-font-size', 12, isFiniteNumber))))
);
watch(fontSize, (v) => writeStorage('st-font-size', v));
// 消息流字体：非法值回退 consolas 预设
export const fontFamily = ref(readStorageChecked<string>('st-font-family', 'consolas', isString));
watch(fontFamily, (v) => writeStorage('st-font-family', v));

// 消息流字体栈：预设键映射；其余值视为系统扫描到的字体家族名直接使用（Consolas 兜底保持等宽对齐）
export const fontFamilyStack = computed(() => {
  const table: Record<string, string> = {
    consolas: "Consolas, 'Courier New', monospace",
    mono: "'Cascadia Mono', 'JetBrains Mono', 'Fira Code', monospace",
    system: "Inter, -apple-system, 'Segoe UI', Arial, sans-serif",
  };
  const v = fontFamily.value;
  if (table[v]) return table[v];
  return v ? `'${v.replace(/'/g, '')}', Consolas, monospace` : table.consolas;
});

// 系统字体列表：Rust 端 DirectWrite 枚举（启动时拉一次），设置字体下拉的数据源
export const systemFonts = ref<string[]>([]);
// 日志导出目录：空 = 应用日志目录（后端 app_log_dir，取不到时回退 exe/log）；设置里可自定义
export const logDir = ref(readStorageChecked<string>('st-log-dir', '', isString));
watch(logDir, (v) => writeStorage('st-log-dir', v));
// 默认导出目录（后端返回应用日志目录），仅作设置输入框的占位提示
export const defaultLogDir = ref('');

// ---------- 发送选项与快捷命令 ----------
// 换行枚举（顺序即“自动换行”按钮的循环顺序）
const NEWLINE_VALUES: SendSettings['newline'][] = ['none', 'lf', 'crlf', 'cr'];
const DEFAULT_SEND_SETTINGS: SendSettings = { newline: 'none', loopSend: false, loopInterval: 1000 };

// 发送设置逐字段校验：newline 非法曾导致发送内容拼出字面量 "undefined"；
// loopInterval 非法会让后端定时线程拿到 NaN（interval 0）。非法字段各自回退默认值。
const sanitizeSendSettings = (value: unknown): SendSettings => {
  const raw = (value && typeof value === 'object' ? value : {}) as Partial<SendSettings>;
  const newline = NEWLINE_VALUES.includes(raw.newline as SendSettings['newline'])
    ? (raw.newline as SendSettings['newline'])
    : DEFAULT_SEND_SETTINGS.newline;
  const interval =
    typeof raw.loopInterval === 'number' && Number.isFinite(raw.loopInterval)
      ? raw.loopInterval
      : DEFAULT_SEND_SETTINGS.loopInterval;
  return {
    newline,
    // 安全默认：自动发送不跨启动保留，每次启动强制关闭
    loopSend: false,
    loopInterval: Math.min(600000, Math.max(10, Math.round(interval))),
  };
};
export const sendSettings = ref<SendSettings>(sanitizeSendSettings(readStorage<unknown>('st-send-settings', null)));
watch(sendSettings, (v) => writeStorage('st-send-settings', v), { deep: true });

// 快捷命令默认项（存储损坏时的回退数据）
const DEFAULT_QUICK_COMMANDS: QuickCommand[] = [
  { name: 'AT', text: 'AT' },
  { name: '版本', text: 'AT+GMR' },
];
// 快捷命令数组校验：非数组回退默认；数组内每项必须是 {name,text} 字符串（空数组视为用户清空，保留）
const sanitizeQuickCommands = (value: unknown): QuickCommand[] => {
  if (!Array.isArray(value)) return DEFAULT_QUICK_COMMANDS.map((cmd) => ({ ...cmd }));
  return value
    .filter(
      (item): item is QuickCommand =>
        !!item &&
        typeof item === 'object' &&
        typeof (item as QuickCommand).name === 'string' &&
        typeof (item as QuickCommand).text === 'string'
    )
    .slice(0, 100)
    .map((item) => ({ name: item.name, text: item.text }));
};
export const quickCommands = ref<QuickCommand[]>(sanitizeQuickCommands(readStorage<unknown>('st-quick-cmds', null)));
watch(quickCommands, (v) => writeStorage('st-quick-cmds', v), { deep: true });

// 自动换行当前项的显示文本（按钮文案用）
export const newlineLabel = computed(() => {
  const map: Record<SendSettings['newline'], string> = {
    none: 'None',
    lf: '\\n',
    crlf: '\\r\\n',
    cr: '\\r',
  };
  return map[sendSettings.value.newline];
});

// 循环切换自动换行：None → LF → CRLF → CR → None
export const cycleNewline = () => {
  const i = NEWLINE_VALUES.indexOf(sendSettings.value.newline);
  sendSettings.value.newline = NEWLINE_VALUES[(i + 1) % NEWLINE_VALUES.length];
};

// 发送框 HEX/文本切换：双向转换内容（字符串⇄HEX 字节流），空内容仅翻转模式。
// 单屏与分屏工具带共用一份实现。
export const toggleSendHex = (session: ConnectionSession) => {
  const toHex = !(session.sendHexMode ?? false);
  session.sendHexMode = toHex;
  if (!session.sendText.trim()) return;
  session.sendText = toHex
    ? bytesToHex(Array.from(new TextEncoder().encode(session.sendText)))
    : new TextDecoder().decode(new Uint8Array(hexToBytes(session.sendText)));
};

// HEX 模式输入过滤：实时剥除非十六进制字符（允许空格分组），并保持光标位置。
// 直接改 el.value + 回写 session.sendText，配合 v-model 双向不冲突。
export const filterHexInput = (session: ConnectionSession, e: Event) => {
  if (!(session.sendHexMode ?? false)) return;
  const el = e.target as HTMLTextAreaElement | HTMLInputElement;
  const filtered = el.value.replace(/[^0-9a-fA-F ]/g, '');
  if (filtered !== el.value) {
    const pos = el.selectionStart;
    el.value = filtered;
    session.sendText = filtered;
    el.setSelectionRange(pos, pos);
  }
};

const newlineSeq = computed(() => {
  const table: Record<SendSettings['newline'], string> = { none: '', lf: '\n', crlf: '\r\n', cr: '\r' };
  return table[sendSettings.value.newline];
});

// 换行符对应的原始字节（HEX 发送模式追加到字节流末尾，文本模式由 TextEncoder 统一编码）
const newlineBytes = () => Array.from(new TextEncoder().encode(newlineSeq.value));

// ---------- 总线过滤 ----------
export const busSessionFilter = ref('all');
export const busDirectionFilter = ref<'all' | MessageDirection>('all');
export const busKeyword = ref('');

// ---------- 派生状态 ----------
export const activeSession = computed(() => sessions.value.find((item) => item.id === activeSessionId.value));
export const splitSessions = computed(() => sessions.value.filter((item) => selectedSessionIds.value.includes(item.id)));
export const connectedCount = computed(() => sessions.value.filter((item) => item.status === 'connected').length);

// ---------- 连接操作进行中标记（防重入） ----------
// 打开/关闭连接是异步 invoke：连点会并发执行，导致后端双开/双关、状态与真实句柄错位。
// 用响应式 Set 记录进行中的会话 id，调用方与按钮据此禁用/直接返回。
export const busySessions = reactive(new Set<string>());
export const isSessionBusy = (id: string) => busySessions.has(id);

// 新建会话按钮的可用性：串口需要选端口，网络需要端口合法（客户端/UDP 还需目标地址）
export const canCreateSession = computed(() => {
  if (newSessionType.value === 'serial') return !!newSessionConfig.value.port;
  const net = newNetConfig.value;
  const portOk = net.port >= 1 && net.port <= 65535;
  const localOk = newSessionType.value !== 'udp' || (net.localPort >= 0 && net.localPort <= 65535);
  const hostOk = newSessionType.value === 'tcp_server' || !!net.host.trim();
  return portOk && localOk && hostOk;
});

// 总线过滤不修改原始日志，只决定当前界面显示哪些消息。
export const filteredGlobalMessages = computed(() => {
  const keyword = busKeyword.value.trim().toLowerCase();

  return globalMessages.value.filter((message) => {
    const matchSession = busSessionFilter.value === 'all' || message.sessionId === busSessionFilter.value;
    const matchDirection = busDirectionFilter.value === 'all' || message.direction === busDirectionFilter.value;
    const matchKeyword =
      !keyword || message.text.toLowerCase().includes(keyword) || message.sessionName.toLowerCase().includes(keyword);
    return matchSession && matchDirection && matchKeyword;
  });
});

// ---------- 内部句柄 ----------
let pollTimer: ReturnType<typeof setInterval>;
let unlistenSerialData: UnlistenFn | undefined;
let unlistenSerialDisconnect: UnlistenFn | undefined;
let unlistenNetData: UnlistenFn | undefined;
let unlistenNetDisconnect: UnlistenFn | undefined;
let unlistenAutoSent: UnlistenFn | undefined;
let unlistenAutoSendStopped: UnlistenFn | undefined;

// ---------- 日志导出 ----------
// 把当前流式消息框按会话当前的显示开关原样导出（所见即所得）。
// 文件名 = 端口号或 IP_端口 + 导出时刻（串口：COM3_…；网络：127.0.0.1_9000_…）。
// 目录 = 设置里配置的日志导出路径；留空 = 应用日志目录（后端 app_log_dir，取不到时回退 exe/log）。
// 内容 = 消息框所见即所导：逐条按入队时刻的开关快照渲染（时间戳/RX/TX 前缀/HEX 与界面
// 显示完全一致，无快照旧消息回退当前开关）；不加任何额外头或统计行。成功弹 toast 告知位置。
export const exportSessionLog = async (session: ConnectionSession) => {
  if (session.messages.length === 0) {
    notify('当前没有消息可导出');
    return;
  }
  const lines = session.messages
    .filter((m) => !m.hidden) // 所见即所导：方向关闭期间隐藏的行不导出
    .map((m) => {
      // 所见即所导：工具带「标签」总开关关闭时，导出内容也不带时间戳/RX/TX 前缀
      const meta = showMessageMeta.value;
      const dir = meta && (m.snap ? (m.direction === 'RX' ? m.snap.rx : m.snap.tx) : true);
      const hex = m.snap ? m.snap.hex : (session.hexMode ?? false);
      let line = '';
      if (meta) line += `${m.time} `;
      if (dir) line += `${m.direction} `;
      line += hex && m.raw ? bytesToHex(m.raw) : m.text;
      return line;
    });
  if (lines.length === 0) {
    notify('当前没有可导出的显示消息（RX/TX 显示开关可能均已关闭）');
    return;
  }
  // 端点标识：串口用端口号（COM3），网络用 IP_端口；时间戳精确到秒，文件名唯一
  const endpoint =
    session.type === 'serial'
      ? session.config.port
      : `${session.net?.host || '0.0.0.0'}_${session.net?.port || 0}`;
  const stamp = `${todayText()}_${nowText().replace(/:/g, '-')}`;
  const filename = `${endpoint}_${stamp}.log`;
  try {
    const path = await invoke<string>('serial_log_write', {
      dir: logDir.value.trim() || null,
      filename,
      text: lines.join(String.fromCharCode(10)) + String.fromCharCode(10),
    });
    notify(`日志已保存到 ${path}`);
  } catch (e) {
    notify(`导出失败: ${e}`);
  }
};

// ---------- 消息写入 ----------
// RX/TX 数据写总线（仅限已加入总览的会话）+ 会话内消息，各自上限 2000 条。
// INFO 系统提示不再入消息流，统一走 notify() 中央通知。
const appendGlobalMessage = (session: ConnectionSession, direction: MessageDirection, text: string) => {
  if (!session.inBus) return; // 总览选择性加入：未加入的会话不汇入总线
  globalMessages.value.push({
    id: `${Date.now()}-${Math.random()}`,
    time: nowText(),
    sessionId: session.id,
    sessionName: session.name,
    direction,
    text,
  });

  if (globalMessages.value.length > 2000) {
    globalMessages.value.splice(0, globalMessages.value.length - 2000);
  }
};

const appendSessionMessage = (session: ConnectionSession, direction: MessageDirection, text: string, bytes?: number[]) => {
  const message: SessionMessage = {
    id: `${Date.now()}-${Math.random()}`,
    time: nowText(),
    direction,
    text,
    raw: bytes,
    // 固化入队时刻的显示开关：HEX/方向前缀只对新数据生效，旧消息保持原样；
    // 方向显示关闭期间入队的行标记 hidden（不补显示：重开后只显示新数据，隐藏期数据不入框）
    snap: {
      rx: session.filterRx ?? true,
      tx: session.filterTx ?? true,
      hex: session.hexMode ?? false,
    },
    hidden: direction === 'RX' ? !(session.filterRx ?? true) : !(session.filterTx ?? true),
  };
  session.messages.push(message);

  if (session.messages.length > 2000) {
    session.messages.splice(0, session.messages.length - 2000);
    // 静默丢弃会让用户误以为数据都在，裁剪时提示一次并指路日志开关
    if (!session.warnedOverflow) {
      session.warnedOverflow = true;
      notify('消息已达 2000 条上限，最早的数据已移出界面；开启「保存日志」可留存完整数据');
    }
  }
};

// ---------- 中央通知（toast） ----------
// 替代消息流内的 INFO 行与状态栏灰字：界面中央弹出、上浮渐隐自动消失。
export const toasts = ref<Array<{ id: number; text: string }>>([]);
let toastSeq = 0;
// 每条 toast 的自动消失定时器：disposeApp 时统一清理，避免热重载/销毁后定时器继续改状态
const toastTimers = new Set<ReturnType<typeof setTimeout>>();

export const notify = (text: string) => {
  const id = ++toastSeq;
  toasts.value.push({ id, text });
  // 最多同时 4 条，超出丢最旧的
  if (toasts.value.length > 4) toasts.value.shift();
  const timer = setTimeout(() => {
    toastTimers.delete(timer);
    const index = toasts.value.findIndex((t) => t.id === id);
    if (index >= 0) toasts.value.splice(index, 1);
  }, 2600);
  toastTimers.add(timer);
};

// ---------- 转发规则 ----------
// 单规则模型：一次只允许一条转发路径，再次"开启"直接覆盖。
const forwardRules = ref<ForwardRule[]>([]);

// 会话关闭/断开时调用：生效中的转发涉及该会话则自动关闭转发
const stopForwardIfInvolved = (sessionId: string) => {
  const rule = forwardRules.value[0];
  if (rule && (rule.fromId === sessionId || rule.toId === sessionId)) {
    forwardRules.value = [];
    const name = sessions.value.find((s) => s.id === sessionId)?.name ?? '';
    notify(`转发已关闭（${name} 已断开）`);
  }
};

export const startForward = (fromId: string, toId: string) => {
  if (!fromId || !toId) {
    notify('请选择转发来源与目标会话');
    return;
  }
  if (fromId === toId) {
    notify('转发来源与目标不能是同一个会话');
    return;
  }
  const fromSession = sessions.value.find((s) => s.id === fromId);
  const toSession = sessions.value.find((s) => s.id === toId);
  if (!fromSession || !toSession) {
    notify('转发涉及的会话已不存在，请重新选择');
    return;
  }
  // 转发只在 RX 时触发且目标必须可写：双方都连接才能开启，避免开启后永远无数据/锁死界面
  if (fromSession.status !== 'connected' || toSession.status !== 'connected') {
    notify(
      `开启转发前请先连接双方会话（${fromSession.name}: ${fromSession.status === 'connected' ? '已连接' : '未连接'}；` +
        `${toSession.name}: ${toSession.status === 'connected' ? '已连接' : '未连接'}）`
    );
    return;
  }
  // 重复拦截：与当前生效规则完全相同的源/目的不重复开启
  const current = forwardRules.value[0];
  if (current && current.fromId === fromId && current.toId === toId) {
    notify('该转发已开启');
    return;
  }
  forwardRules.value = [{ id: `fr-${Date.now()}`, fromId, toId }];
  notify(`转发已开启：${fromSession.name} → ${toSession.name}`);
};

export const stopForward = () => {
  forwardRules.value = [];
  notify('转发已关闭');
};

// 当前生效中的转发规则（转发视图按钮状态与提示用）
export const activeForwardRule = computed(() => forwardRules.value[0] ?? null);

// 转发记录：每条转发的时间、路径与数据内容（转发视图下方实时列表），上限 500 条
export const forwardLogs = ref<Array<{ id: string; time: string; fromName: string; toName: string; text: string }>>([]);

export const clearForwardLogs = () => {
  forwardLogs.value = [];
};

// ---------- 选项刷新 ----------
export const refreshPorts = async () => {
  try {
    const list = await invoke<SerialPortInfo[]>('serial_list_ports');
    // 内容无变化时不更新引用，避免每 5 秒的轮询触发无谓的重渲染（也是“卡顿感”的来源之一）
    const changed =
      list.length !== ports.value.length ||
      list.some((p, i) => p.portnum !== ports.value[i]?.portnum || p.portproduct !== ports.value[i]?.portproduct);
    if (changed) ports.value = list;
    if (!newSessionConfig.value.port && ports.value.length > 0) {
      newSessionConfig.value.port = ports.value[0].portnum;
    }
  } catch (e) {
    console.error('获取串口失败:', e);
  }
};

const refreshOptions = async () => {
  try {
    baudRates.value = await invoke<number[]>('serial_baudrate_list');
    dataBits.value = await invoke<number[]>('serial_databit_list');
    parityBits.value = await invoke<string[]>('serial_paritybit_list');
    stopBits.value = await invoke<number[]>('serial_stopbit_list');
    localIps.value = await invoke<string[]>('net_local_ips');
  } catch (e) {
    console.error('获取下拉选项失败:', e);
  }
};

// ---------- 自定义波特率 ----------
export const addCustomBaudRate = (raw: string) => {
  const val = parseInt(raw, 10);
  if (isNaN(val) || val <= 0) return false;

  if (!baudRates.value.includes(val)) {
    baudRates.value.push(val);
    baudRates.value.sort((a, b) => a - b);
  }

  newSessionConfig.value.baudRate = val;
  return true;
};

// ---------- 会话管理 ----------
// 创建会话只是把配置加入前端列表，不会马上占用端口。
// 用户点击“打开连接”时，才会通过 invoke 调用 Rust 后端真正建立连接。
export const createSession = () => {
  if (!canCreateSession.value) return;

  const isSerial = newSessionType.value === 'serial';
  // 串口同端口再添加：不新建——把表单中的新参数（波特率等）套用到既有会话上。
  // 连接中：断开 → 更新参数 → 用新参数重连；未连接：直接更新配置；参数相同仅选中该会话。
  if (isSerial) {
    const existing = sessions.value.find(
      (item) => item.type === 'serial' && item.config.port === newSessionConfig.value.port
    );
    if (existing) {
      // 连接/断开进行中修改参数会与异步流程交错（配置被半途替换），先拒绝
      if (busySessions.has(existing.id)) {
        notify(`${existing.name} 正在连接/断开中，请稍候再修改参数`);
        return;
      }
      const cfg = { ...newSessionConfig.value };
      const same =
        existing.config.baudRate === cfg.baudRate &&
        existing.config.dataBits === cfg.dataBits &&
        existing.config.parityBits === cfg.parityBits &&
        existing.config.stopBits === cfg.stopBits;
      if (same) {
        activeSessionId.value = existing.id;
        notify(`${cfg.port} 会话已存在且参数相同`);
        return;
      }
      const wasConnected = existing.status === 'connected';
      void (async () => {
        if (wasConnected) {
          const closed = await closeConnection(existing);
          // 每个 await 后校验会话仍在列表中（用户可能已删除），关闭失败则放弃参数更新
          if (!sessions.value.some((item) => item.id === existing.id)) return;
          if (!closed) {
            notify(`${cfg.port} 参数更新已取消：关闭连接失败`);
            return;
          }
        }
        // 后端 serial_close 已 join 读线程并释放句柄，无需固定延时即可重连
        existing.config = cfg;
        if (wasConnected) {
          const opened = await openConnection(existing);
          if (!sessions.value.some((item) => item.id === existing.id)) return;
          if (!opened) {
            notify(`${cfg.port} 参数已更新，但重新连接失败`);
            return;
          }
        }
        activeSessionId.value = existing.id;
        notify(`${cfg.port} 参数已更新${wasConnected ? '并重新连接' : ''}`);
      })();
      return;
    }
  }
  // 网络不重复创建：同类型 + 完全相同的地址/端口配置视为同一会话
  if (!isSerial) {
    const net = newNetConfig.value;
    const dup = sessions.value.find(
      (item) =>
        item.type === newSessionType.value &&
        item.net &&
        item.net.host === net.host &&
        item.net.port === net.port &&
        item.net.localPort === net.localPort &&
        item.net.localHost === net.localHost
    );
    if (dup) {
      notify(`${dup.name} 已存在连接会话，不能重复创建`);
      return;
    }
  }
  // 命名：仅在列表里已存在"同名"会话时才追加 #2/#3 递增后缀；
  // 删除旧会话后名字会被回收复用（按名字查重，而非按端口计数）
  const baseName = isSerial
    ? newSessionConfig.value.port
    : netSessionName(newSessionType.value, newNetConfig.value);
  let name = baseName;
  let suffix = 2;
  while (sessions.value.some((item) => item.name === name)) {
    name = `${baseName} #${suffix++}`;
  }

  const session: ConnectionSession = isSerial
    ? {
        id: `serial-${newSessionConfig.value.port}-${Date.now()}`,
        name,
        type: 'serial',
        status: 'closed',
        config: { ...newSessionConfig.value },
        sendText: '',
        messages: [],
        messageCount: 0,
        txBytes: 0,
        rxBytes: 0,
        statusMsg: '已创建，等待打开',
        filterRx: true,
        filterTx: true,
      }
    : {
        id: `net-${newSessionType.value}-${Date.now()}`,
        name,
        type: newSessionType.value,
        status: 'closed',
        config: { ...newSessionConfig.value },
        net: { ...newNetConfig.value },
        sendText: '',
        messages: [],
        messageCount: 0,
        txBytes: 0,
        rxBytes: 0,
        statusMsg: '已创建，等待打开',
        filterRx: true,
        filterTx: true,
      };

  crumb('session create ' + session.name);
  sessions.value.push(session);
  // 关键：从响应式数组取回代理对象再使用——
  // 局部变量 session 是原始对象，直接改它的 status 不会触发 UI 更新（状态不同步 bug 的根因）
  const stored = sessions.value[sessions.value.length - 1];
  activeSessionId.value = stored.id;
  // 分屏默认不加入：需要时在卡片上手动勾选"分屏"

  // 添加后默认自动打开连接（用户预期：建好即用）
  void openConnection(stored);
};

export const removeSession = async (session: ConnectionSession) => {
  // 删除会话（卡片与详情页均始终可见）：连接中先走关闭流程，成功后再移除；
  // 关闭失败则保留会话（端口仍被占用，移出列表会让用户看不到真实状态）
  if (busySessions.has(session.id)) {
    notify(`${session.name} 正在连接/断开中，请稍候再删除`);
    return;
  }
  if (session.status === 'connected') {
    const closed = await closeConnection(session);
    if (!sessions.value.some((item) => item.id === session.id)) return; // 等待期间已被并发移除
    if (!closed) {
      // 关闭失败时保留会话：端口仍被占用，移出列表会让用户看不到真实状态
      stopForwardIfInvolved(session.id);
      notify(`关闭连接失败，未移除会话（${session.name}）`);
      return;
    }
  }
  // 无论如何处理转发联动（关闭成功 / 会话本就未连接）
  stopForwardIfInvolved(session.id);
  // 该会话正在自动发送：先停后端线程并解除锁定，再移除，避免线程继续写已删除的会话
  if (loopSessionId.value === session.id) {
    await stopLoop();
    sendSettings.value.loopSend = false;
  }
  // 丢弃该会话尚未 flush 的合帧缓冲，避免删除后又冒出迟到消息
  pendingRx.delete(session.id);
  pendingTx.delete(session.id);
  sessions.value = sessions.value.filter((item) => item.id !== session.id);
  selectedSessionIds.value = selectedSessionIds.value.filter((id) => id !== session.id);

  if (activeSessionId.value === session.id) {
    activeSessionId.value = sessions.value[0]?.id ?? '';
  }
};

export const toggleSplitSession = (sessionId: string) => {
  if (selectedSessionIds.value.includes(sessionId)) {
    selectedSessionIds.value = selectedSessionIds.value.filter((id) => id !== sessionId);
  } else {
    selectedSessionIds.value.push(sessionId);
  }
};

// ---------- 打开 / 关闭 ----------
// 打开成功返回 true、失败返回 false（调用方据此决定是否继续后续动作）
const openSerial = async (session: ConnectionSession) => {
  try {
    const result = await invoke<string>('serial_open', {
      port: session.config.port,
      baudRate: session.config.baudRate,
      dataBits: session.config.dataBits,
      parityBits: session.config.parityBits,
      stopBits: session.config.stopBits,
    });
    session.status = 'connected';
    session.statusMsg = result;
    // 新连接从干净的解码状态开始（旧连接可能残留未完成的 UTF-8 半字符）
    resetDecoders(session);
    notify(result);
    return true;
  } catch (e) {
    session.statusMsg = `打开失败: ${e}`;
    notify(session.statusMsg);
    return false;
  }
};

// 打开网络会话：key 用会话 id，服务端监听/UDP 绑定/客户端连接参数都来自会话配置。
const openNet = async (session: ConnectionSession) => {
  try {
    const result = await invoke<string>('net_open', {
      key: session.id,
      kind: session.type,
      host: session.net?.host ?? '',
      port: session.net?.port ?? 0,
      localPort: session.net?.localPort ?? 0,
      localHost: session.net?.localHost ?? '',
    });
    session.status = 'connected';
    session.statusMsg = result;
    resetDecoders(session);
    notify(result);
    return true;
  } catch (e) {
    session.statusMsg = `打开失败: ${e}`;
    notify(session.statusMsg);
    return false;
  }
};

// 按会话类型分发打开动作：统一入口带防重入，连点/自动打开与手动打开并发时直接返回，
// 避免后端对同一端口重复打开（双开/句柄覆盖）。
export const openConnection = async (session: ConnectionSession): Promise<boolean> => {
  if (session.status === 'connected') return true;
  if (busySessions.has(session.id)) {
    notify(`${session.name} 正在连接…`);
    return false;
  }
  busySessions.add(session.id);
  try {
    return session.type === 'serial' ? await openSerial(session) : await openNet(session);
  } finally {
    busySessions.delete(session.id);
  }
};

const closeSerial = async (session: ConnectionSession) => {
  try {
    const result = await invoke<string>('serial_close', { port: session.config.port });
    session.status = 'closed';
    session.statusMsg = result;
    // 丢弃残留的 UTF-8 半字符，重开后不会与旧数据拼接
    resetDecoders(session);
    // 会话关闭联动：生效中的转发涉及该会话则自动关闭
    stopForwardIfInvolved(session.id);
    // 后端已在 close 时停止自动发送——前端锁定同步解除，避免"重开后自动发送静默失效"
    releaseLoopFor(session.id);
    notify(result);
    return true;
  } catch (e) {
    session.statusMsg = `关闭失败: ${e}`;
    notify(session.statusMsg);
    return false;
  }
};

// 关闭网络会话：Rust 侧 shutdown 后读线程退出
const closeNet = async (session: ConnectionSession) => {
  try {
    const result = await invoke<string>('net_close', { key: session.id });
    session.status = 'closed';
    session.statusMsg = result;
    resetDecoders(session);
    stopForwardIfInvolved(session.id);
    releaseLoopFor(session.id);
    notify(result);
    return true;
  } catch (e) {
    session.statusMsg = `关闭失败: ${e}`;
    notify(session.statusMsg);
    return false;
  }
};

// 按会话类型分发关闭动作：同样防重入（断开过程中再次点击不会重复 invoke）
export const closeConnection = async (session: ConnectionSession): Promise<boolean> => {
  if (session.status !== 'connected') return true; // 已关闭视为成功
  if (busySessions.has(session.id)) {
    notify(`${session.name} 正在断开…`);
    return false;
  }
  busySessions.add(session.id);
  try {
    return session.type === 'serial' ? await closeSerial(session) : await closeNet(session);
  } finally {
    busySessions.delete(session.id);
  }
};

// 卡片上的连接开关：已连接则关闭，已关闭则重新打开
export const toggleConnection = (session: ConnectionSession) =>
  session.status === 'connected' ? closeConnection(session) : openConnection(session);

// ---------- 发送 ----------
// 发送载荷统一构建：文本模式 = 原文 + 换行符（UTF-8 编码）；HEX 模式 = HEX 解析字节 + 换行字节。
// 返回 { text, bytes }：text 用于消息流展示与后端 data 兜底，bytes 为实际写出的原始字节。
const buildPayloadFrom = (session: ConnectionSession, raw: string): { text: string; bytes: number[] } => {
  if (session.sendHexMode ?? false) {
    const bytes = hexToBytes(raw).concat(newlineBytes());
    return { text: bytesToHex(bytes), bytes };
  }
  const text = raw + newlineSeq.value;
  return { text, bytes: Array.from(new TextEncoder().encode(text)) };
};

// 发送框内容对应的载荷（手动发送与自动发送共用同一实现，保证两条路径字节完全一致）
export const buildSendPayload = (session: ConnectionSession) => buildPayloadFrom(session, session.sendText);

// 同一会话的写操作串行化：Tauri 异步命令各自进入 spawn_blocking 线程池，
// 连续 invoke 不保证执行顺序；转发/高频发送必须严格按产生顺序写出，否则字节流乱序。
// 队列链上单次失败不能中断后续写入（prev.catch），调用方仍能拿到本次错误。
let writeQueues = new WeakMap<ConnectionSession, Promise<unknown>>();
const queueSessionWrite = <T>(session: ConnectionSession, task: () => Promise<T>): Promise<T> => {
  const prev = writeQueues.get(session) ?? Promise.resolve();
  const next = prev.catch(() => {}).then(task);
  writeQueues.set(session, next);
  return next;
};

// 发送入口统一走这里：构建载荷、写入后端、写入总线/会话流。
// textOverride 用于快捷命令与循环发送，此时不触发“发送后清空”。
export const sendData = async (session: ConnectionSession, textOverride?: string) => {
  const raw = textOverride ?? session.sendText;
  if (!raw) return;

  if (session.status !== 'connected') {
    session.statusMsg = '请先打开连接再发送数据';
    notify(session.statusMsg);
    return;
  }

  const payload = buildPayloadFrom(session, raw);
  try {
    // 始终传 bytes：后端优先使用原始字节，避免文本再次编码造成字节偏移
    await queueSessionWrite(session, () =>
      session.type === 'serial'
        ? invoke('serial_write', { port: session.config.port, data: payload.text, bytes: payload.bytes })
        : invoke('net_write', { key: session.id, data: payload.text, bytes: payload.bytes })
    );
    appendGlobalMessage(session, 'TX', payload.text);
    appendSessionMessage(session, 'TX', payload.text, payload.bytes);
    session.messageCount += 1;
    session.txBytes += payload.bytes.length;
  } catch (e) {
    session.statusMsg = `发送失败: ${e}`;
    notify(session.statusMsg);
  }
};

export const clearSessionReceive = (session: ConnectionSession) => {
  session.messages = [];
  session.messageCount = 0;
  // 清空同时归零收发字节计数（隐藏会话不经过此路径，计数保留）
  session.rxBytes = 0;
  session.txBytes = 0;
  // 丢弃合帧缓冲与流式解码残留：清空后不再冒出迟到数据，也不会与旧半字符拼接
  pendingRx.delete(session.id);
  pendingTx.delete(session.id);
  resetDecoders(session);
};

// ---------- 消息转发 ----------
// 按转发界面配置的规则执行：来源会话收到的数据按原始字节透传发往目标会话（跨串口/网络均可）。
// 只在收到数据（RX）时触发；目标会话必须已连接。转发本身不会再次引发转发，天然无环路。
// 注意：不经过 sendData（那会追加换行符并把字节重新按文本编码），这里保证字节级一致。
const sendForwarded = async (target: ConnectionSession, text: string, bytes: number[]) => {
  try {
    // 转发同样走会话写队列：同帧多个来源段按到达顺序写出，避免异步命令乱序
    await queueSessionWrite(target, () =>
      target.type === 'serial'
        ? invoke('serial_write', { port: target.config.port, data: text, bytes })
        : invoke('net_write', { key: target.id, data: text, bytes })
    );
    // 转发写出的数据同样记入目标会话的 TX 流与字节统计
    appendGlobalMessage(target, 'TX', text);
    appendSessionMessage(target, 'TX', text, bytes);
    target.messageCount += 1;
    target.txBytes += bytes.length;
  } catch (e) {
    target.statusMsg = `转发失败: ${e}`;
    notify(target.statusMsg);
  }
};

const forwardIfConfigured = (session: ConnectionSession, text: string, bytes?: number[]) => {
  // 缺原始字节的兜底路径：按文本 UTF-8 编码转发（正常 RX 事件始终带 bytes）
  const payload = bytes && bytes.length > 0 ? bytes : Array.from(new TextEncoder().encode(text));
  if (payload.length === 0) return; // 空数据不触发写操作（避免无意义的 IPC 与转发日志）
  for (const rule of forwardRules.value) {
    if (rule.fromId !== session.id) continue;
    const target = sessions.value.find((item) => item.id === rule.toId);
    if (target && target.id !== session.id && target.status === 'connected') {
      void sendForwarded(target, text, payload);
      forwardLogs.value.push({
        id: `${Date.now()}-${Math.random()}`,
        time: nowText(),
        fromName: session.name,
        toName: target.name,
        text,
      });
      if (forwardLogs.value.length > 500) {
        forwardLogs.value.splice(0, forwardLogs.value.length - 500);
      }
    }
  }
};

// ---------- 快捷命令 ----------
export const addQuickCommand = () => {
  quickCommands.value.push({ name: `命令${quickCommands.value.length + 1}`, text: '' });
};

export const removeQuickCommand = (index: number) => {
  quickCommands.value.splice(index, 1);
};

export const sendQuickCommand = (cmd: QuickCommand) => {
  const session = activeSession.value;
  if (!session) return;
  void sendData(session, cmd.text);
};

// ---------- 自动发送（后端定时线程） ----------
// 勾选「自动」后由 Rust 侧专一线程按间隔直接写连接，绕过前端 IPC——
// 高频发送（如 10ms）不再造成 IPC 堆积与 UI 卡顿。
// 目标锁定：开启时记录 loopSessionId，之后切换视图/会话都不会静默改变发送目标（杜绝误发）。
export const loopSessionId = ref<string>('');
// 当前自动发送的目标会话名（单屏按钮标题、分屏面板提示用；未开启时为空串）
export const loopTargetName = computed(
  () => sessions.value.find((item) => item.id === loopSessionId.value)?.name ?? ''
);
// requestLoopFor 指定的下一次目标：loopSend 由 false→true 前先记录，watcher 再消费
let pendingLoopTarget = '';
// 当前后端线程实际使用的载荷指纹（内容/HEX 模式/换行）：相同则不重复重启，避免 watch 自激
let lastLoopContentKey = '';
// 重启序号：并发 startLoop 时只有最后一次能真正落到后端
let loopRestartSeq = 0;
let sendTextSyncTimer: ReturnType<typeof setTimeout> | undefined;

// 目标解析顺序：显式传入 → pending → 当前锁定 → 当前激活会话；一旦锁定绝不跟随 activeSession 切换
const resolveLoopTarget = (targetId?: string) => {
  const id = targetId || pendingLoopTarget || loopSessionId.value || activeSessionId.value;
  return sessions.value.find((item) => item.id === id);
};

// 载荷指纹（无目标返回空串）
const loopContentKeyOf = (session: ConnectionSession) =>
  `${session.sendText}\u0000${session.sendHexMode ? 1 : 0}\u0000${sendSettings.value.newline}`;
const currentLoopContentKey = () => {
  const id = loopSessionId.value;
  if (!id) return '';
  const session = sessions.value.find((item) => item.id === id);
  return session ? loopContentKeyOf(session) : '';
};

// 停止后端定时线程并解除前端锁定（不修改 loopSend：由调用方决定是否弹回开关）
const stopLoop = async () => {
  // 取消待执行的“内容变化重启”，避免停止后又被旧定时器拉起
  if (sendTextSyncTimer) {
    clearTimeout(sendTextSyncTimer);
    sendTextSyncTimer = undefined;
  }
  const id = loopSessionId.value;
  loopSessionId.value = '';
  lastLoopContentKey = '';
  if (!id) return;
  const target = sessions.value.find((item) => item.id === id);
  if (!target) return;
  try {
    if (target.type === 'serial') {
      await invoke('serial_auto_send_stop', { port: target.config.port });
    } else {
      await invoke('net_auto_send_stop', { key: target.id });
    }
  } catch {
    /* 会话可能已关闭，忽略 */
  }
};

// 连接关闭/断开/删除时立即解除该会话的自动发送锁定（后端线程已随连接停止，无需再发停止命令）
const releaseLoopFor = (sessionId: string) => {
  if (loopSessionId.value !== sessionId) return;
  loopSessionId.value = '';
  lastLoopContentKey = '';
  sendSettings.value.loopSend = false;
};

const startLoop = (targetId?: string) => {
  const seq = ++loopRestartSeq;
  void (async () => {
    const target = resolveLoopTarget(targetId);
    pendingLoopTarget = '';
    // 先停旧线程（目标可能已切换），再按新目标重启
    await stopLoop();
    if (seq !== loopRestartSeq) return; // 已有更新的重启请求，交给后来者
    if (!sendSettings.value.loopSend) return; // 等待期间被用户关闭
    if (!target || !sessions.value.some((item) => item.id === target.id)) {
      sendSettings.value.loopSend = false;
      notify('自动发送未启动：未选择会话');
      return;
    }
    if (target.status !== 'connected') {
      sendSettings.value.loopSend = false;
      notify(`自动发送未启动：${target.name} 未连接`);
      return;
    }
    const payload = buildSendPayload(target);
    if (payload.bytes.length === 0) {
      sendSettings.value.loopSend = false;
      notify('自动发送未启动：发送内容为空');
      return;
    }
    // 先记录锁定目标与载荷指纹，再赋值 loopSessionId（watch 异步 flush，比对指纹即可去重）
    lastLoopContentKey = loopContentKeyOf(target);
    loopSessionId.value = target.id;
    const intervalMs = Math.min(600000, Math.max(10, Math.round(sendSettings.value.loopInterval || 1000)));
    try {
      // data 传文本（后端兜底），bytes 为真实字节：HEX 模式同样按原始字节发送
      if (target.type === 'serial') {
        await invoke('serial_auto_send_start', {
          port: target.config.port,
          data: payload.text,
          bytes: payload.bytes,
          intervalMs,
        });
      } else {
        await invoke('net_auto_send_start', {
          key: target.id,
          data: payload.text,
          bytes: payload.bytes,
          intervalMs,
        });
      }
    } catch (e) {
      loopSessionId.value = '';
      lastLoopContentKey = '';
      sendSettings.value.loopSend = false;
      target.statusMsg = `自动发送启动失败: ${e}`;
      notify(target.statusMsg);
    }
  })();
};

// 自动发送按钮/面板的统一切换入口：已对该会话循环则停止，否则锁定该会话并开启
export const requestLoopFor = (session: ConnectionSession) => {
  if (sendSettings.value.loopSend && loopSessionId.value === session.id) {
    sendSettings.value.loopSend = false; // watcher 负责停后端线程并清锁定
    return;
  }
  if (session.status !== 'connected') {
    notify(`自动发送不可用：${session.name} 未连接`);
    return;
  }
  pendingLoopTarget = session.id;
  if (sendSettings.value.loopSend) {
    // 已在循环其他会话：直接切到新目标（loopSend 保持亮起，手动重启一次）
    startLoop(session.id);
  } else {
    sendSettings.value.loopSend = true; // watcher → startLoop 消费 pendingLoopTarget
  }
};

// 发送框内容/HEX 模式/换行变化：只监听“当前锁定目标”，防抖 300ms 同步到后端线程（始终发最新内容）。
// 注意：目标切换/重启也会触发本 watch，用载荷指纹去重，避免无限重启。
watch(currentLoopContentKey, () => {
  if (!sendSettings.value.loopSend || !loopSessionId.value) return;
  if (sendTextSyncTimer) clearTimeout(sendTextSyncTimer);
  sendTextSyncTimer = setTimeout(() => {
    if (!sendSettings.value.loopSend || !loopSessionId.value) return;
    if (currentLoopContentKey() === lastLoopContentKey) return; // 仅目标切换引起的空转
    startLoop();
  }, 300);
});

watch(
  () => sendSettings.value.loopSend,
  (on) => (on ? startLoop() : void stopLoop())
);

watch(
  () => sendSettings.value.loopInterval,
  () => {
    if (sendSettings.value.loopSend && loopSessionId.value) startLoop();
  }
);

// ---------- 生命周期 ----------
const onSystemThemeChange = (e: MediaQueryListEvent) => {
  systemDark.value = e.matches;
};

// ---------- 流式 UTF-8 解码 ----------
// 一次 read/auto-sent 事件的字节可能恰好切断多字节字符（中文 3 字节、emoji 4 字节）。
// 每会话 RX/TX 各持一个流式 TextDecoder：跨事件续接半字符；关闭/断开/清空时重置，旧残留直接丢弃。
let decoderMap = new WeakMap<ConnectionSession, { rx: TextDecoder; tx: TextDecoder }>();
const decodersOf = (session: ConnectionSession) => {
  let decoders = decoderMap.get(session);
  if (!decoders) {
    decoders = { rx: new TextDecoder('utf-8'), tx: new TextDecoder('utf-8') };
    decoderMap.set(session, decoders);
  }
  return decoders;
};
const decodeRxStream = (session: ConnectionSession, data: number[]) =>
  decodersOf(session).rx.decode(new Uint8Array(data), { stream: true });
const decodeTxStream = (session: ConnectionSession, data: number[]) =>
  decodersOf(session).tx.decode(new Uint8Array(data), { stream: true });
// 重置该会话的解码器（关闭/断开/清空/重开时调用：旧半字符不得与新数据拼接）
const resetDecoders = (session: ConnectionSession) => {
  decoderMap.delete(session);
};

// ---------- 接收合帧 ----------
// 高波特率下 RX 事件频率可达每秒数百次，逐条渲染会造成卡顿；
// 按会话累积文本，16ms（一帧）批量刷入消息流——显示粒度极限，转发也随之整批进行。
// raw 为原始数据（用于转发透传），prefix 为显示前缀（如 TCP 来源地址），二者分离保证转发不带显示标记。
// 字节用 chunks 分块收集、flush 时一次拼接：此前 concat 每批全量拷贝，突发大流量下 O(n²)。
// 按 prefix 分段：同一帧里 TCP 服务端/UDP 多个来源的地址前缀各成一段，不会互相覆盖丢失。
type RxSegment = { prefix: string; raw: string; chunks?: number[][] };
const pendingRx = new Map<string, { session: ConnectionSession; segments: RxSegment[] }>();
let rxFlushScheduled = false;

// 合并分块字节（单块直接复用，避免每帧全量拷贝）
const mergeChunks = (chunks?: number[][]): number[] | undefined => {
  if (!chunks || chunks.length === 0) return undefined;
  if (chunks.length === 1) return chunks[0];
  const bytes: number[] = [];
  for (const chunk of chunks) for (const b of chunk) bytes.push(b);
  return bytes;
};

// 合帧调度与渲染帧对齐（rAF）：延迟稳定为一帧、无定时器精度抖动
// （Windows 定时器精度 ~15.6ms，setTimeout(16) 实际落点会抖）；后台标签页 rAF 自动暂停，
// 回前台立即补发，与原定时器方案的后台行为等价
const scheduleRxFlush = () => {
  if (rxFlushScheduled) return;
  rxFlushScheduled = true;
  // 前台：渲染帧对齐（最低延迟）；后台/最小化窗口 rAF 会暂停——
  // 补 250ms 兜底定时器（先到先执行并清标志，后到的对空缓冲空转，无害）
  requestAnimationFrame(() => flushPendingRx());
  setTimeout(() => flushPendingRx(), 250);
};

const flushPendingRx = () => {
  rxFlushScheduled = false;
  try {
    for (const { session, segments } of pendingRx.values()) {
      for (const segment of segments) {
        const bytes = mergeChunks(segment.chunks);
        const display = segment.prefix + segment.raw;
        session.messageCount += 1;
        appendGlobalMessage(session, 'RX', display);
        appendSessionMessage(session, 'RX', display, bytes);
        // 转发按原始字节透传（显示前缀不随转发带走）
        forwardIfConfigured(session, segment.raw, bytes);
        // 插件管线与消息流同帧合批：每会话每帧至多派发一次，
        // 高波特率下逐事件派发会让订阅插件（如 Hello 的整段重渲染）把主线程打死。
        // 插件异常必须隔离：一个插件抛错不能打断合帧，也不能导致数据重复入队。
        if (bytes && bytes.length > 0) {
          try {
            dispatchRawData(session.id, session.name, bytes);
          } catch (e) {
            console.warn('[dispatch] 插件数据处理异常', e);
            crumb('dispatch-error ' + session.name);
          }
        }
      }
    }
  } finally {
    // 无论如何清空缓冲：异常路径也不允许残留（残留会在下一帧重复入队）
    pendingRx.clear();
  }
};

const enqueueRx = (session: ConnectionSession, raw: string, prefix = '', bytes?: number[]) => {
  let item = pendingRx.get(session.id);
  if (!item) {
    item = { session, segments: [] };
    pendingRx.set(session.id, item);
  }
  const last = item.segments[item.segments.length - 1];
  // 连续同前缀合并为一段；来源变化（TCP 服务端多客户端/UDP 多来源）另起一段，前缀不会互相覆盖
  if (last && last.prefix === prefix) {
    last.raw += raw;
    if (bytes) (last.chunks ??= []).push(bytes);
  } else {
    item.segments.push({ prefix, raw, chunks: bytes ? [bytes] : undefined });
  }
  scheduleRxFlush();
};

// TX 合帧：自动发送由 Rust 线程直接写出（绕过前端 sendData），通过 auto-sent 事件回填 TX 记录
// 字节同样 chunks 分块收集（同 enqueueRx，防 concat O(n²)）
const pendingTx = new Map<string, { session: ConnectionSession; raw: string; chunks?: number[][] }>();
let txFlushScheduled = false;

// TX 合帧同样 rAF 对齐（与 RX 同帧节奏）
const scheduleTxFlush = () => {
  if (txFlushScheduled) return;
  txFlushScheduled = true;
  // 同 scheduleRxFlush：rAF + 后台兜底
  requestAnimationFrame(() => flushPendingTx());
  setTimeout(() => flushPendingTx(), 250);
};

const flushPendingTx = () => {
  try {
    for (const { session, raw, chunks } of pendingTx.values()) {
      const bytes = mergeChunks(chunks);
      session.messageCount += 1;
      appendGlobalMessage(session, 'TX', raw);
      appendSessionMessage(session, 'TX', raw, bytes);
    }
  } finally {
    // 无论如何重置调度标记并清空缓冲（异常路径不残留、不重复入队）
    txFlushScheduled = false;
    pendingTx.clear();
  }
};

const enqueueTx = (session: ConnectionSession, raw: string, bytes?: number[]) => {
  let item = pendingTx.get(session.id);
  if (!item) {
    item = { session, raw: '', chunks: undefined };
    pendingTx.set(session.id, item);
  }
  item.raw += raw;
  if (bytes) (item.chunks ??= []).push(bytes);
  scheduleTxFlush();
};

const findSerialSessionByPort = (port: string) =>
  sessions.value.find((item) => item.type === 'serial' && item.config.port === port);

// ---------- 插件发送直通（'send' 权限的落点） ----------
// 与 sendData 的差异：插件给的是原始字节，无文本编码环节；TX 记录按 HEX 呈现。
const sendRawBytes = async (sessionId: string, bytes: number[]) => {
  const session = sessions.value.find((item) => item.id === sessionId);
  if (!session) throw new Error('会话不存在');
  if (session.status !== 'connected') throw new Error('会话未连接');
  await queueSessionWrite(session, () =>
    session.type === 'serial'
      ? invoke('serial_write', { port: session.config.port, data: '', bytes })
      : invoke('net_write', { key: session.id, data: '', bytes })
  );
  session.txBytes += bytes.length;
  session.messageCount += 1;
  appendSessionMessage(session, 'TX', bytesToHex(bytes), bytes);
};

// 装配事件监听与定时器，App.vue 在 onMounted 调用一次。
export const initApp = async () => {
  // 插件宿主服务桥：必须在任何数据事件可能到达之前就绪
  const sessionSnapshots = () =>
    sessions.value.map((s) => ({ id: s.id, name: s.name, type: s.type, status: s.status }));
  setAppBridge({
    sessions: sessionSnapshots,
    onSessions: (cb) =>
      // 以"会话 id:状态"拼串为键做浅监听：新建/断开/更名都会触发，深度对象变化不误触
      watch(() => sessions.value.map((s) => `${s.id}:${s.status}:${s.name}`).join('|'), () => cb(sessionSnapshots())),
    sendRaw: sendRawBytes,
    notify,
  });

  // 清理后端残留连接：页面重载/HMR 会清空前端会话列表，但后端的串口句柄与
  // 网络监听不会自动释放——不清理会出现"明明没使用却绑定失败 (10048)"的假占用
  await invoke('serial_close_all').catch(() => {});
  await invoke('net_close_all').catch(() => {});

  // system 模式下跟随系统深浅色变化。
  systemDarkQuery.addEventListener('change', onSystemThemeChange);

  await refreshOptions();
  await refreshPorts();

  // 系统字体与默认日志目录：启动拉一次；浏览器调试环境无 Tauri API，静默跳过
  try {
    systemFonts.value = (await invoke<string[]>('system_fonts_list')) ?? [];
  } catch {
    /* 非桌面环境：字体下拉退回三个预设项 */
  }
  try {
    defaultLogDir.value = (await invoke<string>('serial_log_dir')) ?? '';
  } catch {
    /* 非桌面环境：占位提示退回通用文案 */
  }

  // serial-read_data 是 Rust 读线程 emit 出来的事件。
  // payload 里带 port，所以前端能知道这段数据属于哪个会话。
  unlistenSerialData = await listen<{ port: string; data: number[] }>('serial-read_data', (event) => {
    const session = findSerialSessionByPort(event.payload.port);
    if (!session) return;

    session.rxBytes += event.payload.data.length;
    // 流式解码：被事件边界切断的 UTF-8 半字符由本会话解码器续接
    const text = decodeRxStream(session, event.payload.data);
    enqueueRx(session, text, '', event.payload.data);
  });

  unlistenSerialDisconnect = await listen<string>('serial-disconnect', (event) => {
    const session = findSerialSessionByPort(event.payload);
    if (!session) return;

    session.status = 'closed';
    session.statusMsg = `串口 ${event.payload} 已断开`;
    resetDecoders(session);
    // 会话断开联动：生效中的转发涉及该会话则自动关闭
    stopForwardIfInvolved(session.id);
    // 若自动发送锁定该会话，断开后解除锁定并弹回开关（后端任务已随 close 停止）
    releaseLoopFor(session.id);
    notify(session.statusMsg);
    // 断开后主动清理 Rust 侧状态表，否则死句柄残留会导致同端口无法重新打开
    invoke('serial_close', { port: session.config.port }).catch(() => {});
  });

  // auto-sent 是自动发送线程每次写出后 emit 的事件——回填 TX 记录（合帧）。
  // key：串口为端口号，网络为会话 id。
  unlistenAutoSent = await listen<{ key: string; data: number[] }>('auto-sent', (event) => {
    const key = event.payload.key;
    const session = findSerialSessionByPort(key) || sessions.value.find((item) => item.id === key);
    if (!session) return;

    session.txBytes += event.payload.data.length;
    enqueueTx(session, decodeTxStream(session, event.payload.data), event.payload.data);
  });

  // auto-send-stopped：自动发送线程写失败退出时上报——弹回开关并提示原因
  unlistenAutoSendStopped = await listen<{ key: string; data: number[] }>('auto-send-stopped', (event) => {
    const key = event.payload.key;
    const session =
      sessions.value.find((item) => item.id === key) ||
      sessions.value.find((item) => item.type === 'serial' && item.config.port === key);
    if (!session) return;
    releaseLoopFor(session.id);
    // 停止原因是一次性诊断文本，直接用一次性解码即可
    const reason = decodeBytes(event.payload.data);
    session.statusMsg = `自动发送已停止：${reason}`;
    notify(session.statusMsg);
  });

  // net-read_data 是网络读线程 emit 出来的事件。
  // key 对应会话 id；from 为来源地址（TCP 服务端/UDP 场景下区分客户端）。
  unlistenNetData = await listen<{ key: string; data: number[]; from: string }>('net-read_data', (event) => {
    const session = sessions.value.find((item) => item.id === event.payload.key);
    if (!session) return;

    session.rxBytes += event.payload.data.length;
    // 流式解码 + 来源前缀：前缀随分段保留，多个客户端同一帧到达也不会互相覆盖
    const text = decodeRxStream(session, event.payload.data);
    const prefix = event.payload.from ? `[${event.payload.from}] ` : '';
    enqueueRx(session, text, prefix, event.payload.data);
  });

  unlistenNetDisconnect = await listen<string>('net-disconnect', (event) => {
    const session = sessions.value.find((item) => item.id === event.payload);
    if (!session) return;

    session.status = 'closed';
    session.statusMsg = `连接已断开（${session.name}）`;
    resetDecoders(session);
    // 会话断开联动：生效中的转发涉及该会话则自动关闭
    stopForwardIfInvolved(session.id);
    // 若自动发送锁定该会话，断开后解除锁定并弹回开关
    releaseLoopFor(session.id);
    notify(session.statusMsg);
    // 断开后主动清理 Rust 侧状态表，否则死句柄残留会导致同 key 无法重新打开
    invoke('net_close', { key: session.id }).catch(() => {});
  });

  pollTimer = setInterval(refreshPorts, 5000);
};

// 清理事件监听与定时器，避免窗口热更新后重复监听同一个事件。
export const disposeApp = () => {
  systemDarkQuery.removeEventListener('change', onSystemThemeChange);
  clearInterval(pollTimer);
  // 停止自动发送线程并解除前端锁定
  void stopLoop();
  if (sendTextSyncTimer) {
    clearTimeout(sendTextSyncTimer);
    sendTextSyncTimer = undefined;
  }
  // toast 自动消失定时器：销毁后不得再改状态
  for (const timer of toastTimers) clearTimeout(timer);
  toastTimers.clear();
  // 刷掉尚未落盘的合帧缓冲，保证最后一批数据不丢（rAF 调度无需清理挂起句柄）
  flushPendingRx();
  flushPendingTx();
  // 重置全部流式解码器、写队列与忙标记，下一次 initApp 从干净状态开始
  decoderMap = new WeakMap();
  writeQueues = new WeakMap();
  busySessions.clear();
  unlistenSerialData?.();
  unlistenSerialDisconnect?.();
  unlistenNetData?.();
  unlistenNetDisconnect?.();
  unlistenAutoSent?.();
  unlistenAutoSendStopped?.();
};
