import { computed, ref, watch } from 'vue';
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
import { readStorage, writeStorage } from '../utils/storage';
import { decodeBytes, nowText, todayText } from '../utils/format';
import { netSessionName } from '../utils/session';

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

// ---------- 主题：浅色 / 深色 / 系统 ----------
// themeMode 是用户的选择，appliedTheme 是最终落到 DOM 上的类名；
// system 模式下监听系统 prefers-color-scheme，跟随系统实时切换。
export const themeMode = ref<ThemeMode>(readStorage<ThemeMode>('st-theme', 'system'));
// 模块加载时立即同步一次系统深浅状态，避免深色系统用户启动时闪一帧浅色
const systemDarkQuery = window.matchMedia('(prefers-color-scheme: dark)');
const systemDark = ref(systemDarkQuery.matches);
export const appliedTheme = computed<import('../types').AppliedTheme>(() =>
  themeMode.value === 'system' ? (systemDark.value ? 'dark' : 'light') : themeMode.value
);
watch(themeMode, (mode) => writeStorage('st-theme', mode));

// ---------- 显示与日志偏好 ----------
export const showTimestamp = ref(readStorage<boolean>('st-timestamp', true));
watch(showTimestamp, (v) => writeStorage('st-timestamp', v));
// 自动滚动默认关闭（安全默认：自动发送/高频模式下用户自己控制是否跟随）
export const autoScroll = ref(readStorage<boolean>('st-autoscroll', false));
watch(autoScroll, (v) => writeStorage('st-autoscroll', v));
// 启动时强制关闭自动滚动（安全默认：不跨启动保留，用户需要时手动勾选）
autoScroll.value = false;
// 消息流字号（Ctrl+滚轮可在接收区实时调整，范围 11-22）
export const fontSize = ref(readStorage<number>('st-font-size', 12));
watch(fontSize, (v) => writeStorage('st-font-size', v));
// 消息流字体
export const fontFamily = ref(readStorage<string>('st-font-family', 'consolas'));
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
// 日志导出目录：空 = exe 所在目录下的 log（安装目录随应用走）；设置里可自定义
export const logDir = ref(readStorage<string>('st-log-dir', ''));
watch(logDir, (v) => writeStorage('st-log-dir', v));
// 默认导出目录（后端返回 exe\log），仅作设置输入框的占位提示
export const defaultLogDir = ref('');

// ---------- 发送选项与快捷命令 ----------
export const sendSettings = ref<SendSettings>(
  readStorage<SendSettings>('st-send-settings', {
    newline: 'none',
    loopSend: false,
    loopInterval: 1000,
  })
);
// 安全默认：自动发送不跨启动保留，每次启动强制关闭
sendSettings.value.loopSend = false;
watch(sendSettings, (v) => writeStorage('st-send-settings', v), { deep: true });

export const quickCommands = ref<QuickCommand[]>(
  readStorage<QuickCommand[]>('st-quick-cmds', [
    { name: 'AT', text: 'AT' },
    { name: '版本', text: 'AT+GMR' },
  ])
);
watch(quickCommands, (v) => writeStorage('st-quick-cmds', v), { deep: true });

export const newlineSeq = computed(() => {
  const table: Record<SendSettings['newline'], string> = { none: '', lf: '\n', crlf: '\r\n', cr: '\r' };
  return table[sendSettings.value.newline];
});

// ---------- 总线过滤 ----------
export const busSessionFilter = ref('all');
export const busDirectionFilter = ref<'all' | MessageDirection>('all');
export const busKeyword = ref('');

// ---------- 派生状态 ----------
export const activeSession = computed(() => sessions.value.find((item) => item.id === activeSessionId.value));
export const splitSessions = computed(() => sessions.value.filter((item) => selectedSessionIds.value.includes(item.id)));
export const connectedCount = computed(() => sessions.value.filter((item) => item.status === 'connected').length);

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
// 目录 = 设置里配置的日志导出路径；留空 = exe 所在目录下的 log（安装目录随应用走）。
export const exportSessionLog = async (session: ConnectionSession) => {
  if (session.messages.length === 0) {
    session.statusMsg = '当前没有消息可导出';
    return;
  }
  const withTs = session.showTimestamp ?? true;
  const lines = session.messages.map((m) => {
    let line = '';
    if (withTs) line += `[${m.time}] `;
    line += `[${m.direction}] `;
    return line + m.text;
  });
  const header = `==== 导出 ${session.name} | ${nowText()} | ${session.messages.length} 条 ====` + String.fromCharCode(10);
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
      text: header + lines.join(String.fromCharCode(10)) + String.fromCharCode(10),
    });
    session.statusMsg = `已导出 ${session.messages.length} 条到 ${path}`;
  } catch (e) {
    session.statusMsg = `导出失败: ${e}`;
  }
};

// ---------- 消息写入 ----------
// RX/TX 数据写总线（仅限已加入总览的会话）+ 会话内消息，各自上限 2000 条。
// INFO 系统提示不再入消息流，统一走 notify() 中央通知。
export const appendGlobalMessage = (session: ConnectionSession, direction: MessageDirection, text: string) => {
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

export const appendSessionMessage = (session: ConnectionSession, direction: MessageDirection, text: string) => {
  const message: SessionMessage = {
    id: `${Date.now()}-${Math.random()}`,
    time: nowText(),
    direction,
    text,
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

export const notify = (text: string) => {
  const id = ++toastSeq;
  toasts.value.push({ id, text });
  // 最多同时 4 条，超出丢最旧的
  if (toasts.value.length > 4) toasts.value.shift();
  setTimeout(() => {
    const index = toasts.value.findIndex((t) => t.id === id);
    if (index >= 0) toasts.value.splice(index, 1);
  }, 2600);
};

// ---------- 转发规则 ----------
// 单规则模型：一次只允许一条转发路径，再次"启动"直接覆盖。
const forwardRules = ref<ForwardRule[]>([]);

export const startForward = (fromId: string, toId: string) => {
  if (!fromId || !toId || fromId === toId) return;
  forwardRules.value = [{ id: `fr-${Date.now()}`, fromId, toId }];
  const from = sessions.value.find((s) => s.id === fromId)?.name ?? fromId;
  const to = sessions.value.find((s) => s.id === toId)?.name ?? toId;
  notify(`转发已启动：${from} → ${to}`);
};

// 转发记录：每条转发的时间、路径与数据内容（转发视图下方实时列表），上限 500 条
export const forwardLogs = ref<Array<{ id: string; time: string; fromName: string; toName: string; text: string }>>([]);

export const clearForwardLogs = () => {
  forwardLogs.value = [];
};

// ---------- 选项刷新 ----------
export const refreshPorts = async () => {
  try {
    const list = await invoke<SerialPortInfo[]>('serial_list_ports');
    // 内容无变化时不更新引用，避免每 2 秒的轮询触发无谓的重渲染（也是“卡顿感”的来源之一）
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

export const refreshOptions = async () => {
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
  // 串口不重复创建：同一端口已有会话时直接提示（端口是独占资源，开两个会话也没有意义）
  if (isSerial && sessions.value.some((item) => item.type === 'serial' && item.config.port === newSessionConfig.value.port)) {
    notify(`${newSessionConfig.value.port} 已存在连接会话，不能重复创建`);
    return;
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
        showTimestamp: true,
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
        showTimestamp: true,
        filterRx: true,
        filterTx: true,
      };

  sessions.value.push(session);
  // 关键：从响应式数组取回代理对象再使用——
  // 局部变量 session 是原始对象，直接改它的 status 不会触发 UI 更新（状态不同步 bug 的根因）
  const stored = sessions.value[sessions.value.length - 1];
  activeSessionId.value = stored.id;
  if (!selectedSessionIds.value.includes(stored.id)) {
    selectedSessionIds.value.push(stored.id);
  }

  // 添加后默认自动打开连接（用户预期：建好即用）
  void openConnection(stored);
};

export const removeSession = (session: ConnectionSession) => {
  if (session.status === 'connected') {
    session.statusMsg = '请先关闭连接，再删除会话';
    notify(session.statusMsg);
    return;
  }

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
    notify(result);
  } catch (e) {
    session.statusMsg = `打开失败: ${e}`;
    notify(session.statusMsg);
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
    notify(result);
  } catch (e) {
    session.statusMsg = `打开失败: ${e}`;
    notify(session.statusMsg);
  }
};

// 按会话类型分发打开动作
export const openConnection = (session: ConnectionSession) =>
  session.type === 'serial' ? openSerial(session) : openNet(session);

const closeSerial = async (session: ConnectionSession) => {
  try {
    const result = await invoke<string>('serial_close', { port: session.config.port });
    session.status = 'closed';
    session.statusMsg = result;
    // 后端已在 close 时停止自动发送——前端勾选同步弹回，避免"重开后自动发送静默失效"
    if (loopSessionId === session.id) {
      sendSettings.value.loopSend = false;
    }
    notify(result);
  } catch (e) {
    session.statusMsg = `关闭失败: ${e}`;
    notify(session.statusMsg);
  }
};

// 关闭网络会话：Rust 侧 stop_flag 置位后读线程自动退出
const closeNet = async (session: ConnectionSession) => {
  try {
    const result = await invoke<string>('net_close', { key: session.id });
    session.status = 'closed';
    session.statusMsg = result;
    if (loopSessionId === session.id) {
      sendSettings.value.loopSend = false;
    }
    notify(result);
  } catch (e) {
    session.statusMsg = `关闭失败: ${e}`;
    notify(session.statusMsg);
  }
};

// 按会话类型分发关闭动作
export const closeConnection = (session: ConnectionSession) =>
  session.type === 'serial' ? closeSerial(session) : closeNet(session);

// 卡片上的连接开关：已连接则关闭，已关闭则重新打开
export const toggleConnection = (session: ConnectionSession) =>
  session.status === 'connected' ? closeConnection(session) : openConnection(session);

// ---------- 发送 ----------
// 发送入口统一走这里：附加换行符、写入总线/会话流、按需落日志。
// textOverride 用于快捷命令与循环发送，此时不触发“发送后清空”。
export const sendData = async (session: ConnectionSession, textOverride?: string) => {
  const raw = textOverride ?? session.sendText;
  if (!raw) return;

  if (session.status !== 'connected') {
    session.statusMsg = '请先打开连接再发送数据';
    notify(session.statusMsg);
    return;
  }

  try {
    if (session.type === 'serial') {
      await invoke('serial_write', {
        port: session.config.port,
        data: raw + newlineSeq.value,
      });
    } else {
      await invoke('net_write', {
        key: session.id,
        data: raw + newlineSeq.value,
      });
    }
    appendGlobalMessage(session, 'TX', raw);
    appendSessionMessage(session, 'TX', raw);
    session.messageCount += 1;
    // 字节统计按实际写出的 UTF-8 长度计（含换行符）
    session.txBytes += new TextEncoder().encode(raw + newlineSeq.value).length;
  } catch (e) {
    session.statusMsg = `发送失败: ${e}`;
    notify(session.statusMsg);
  }
};

export const clearSessionReceive = (session: ConnectionSession) => {
  session.messages = [];
  session.messageCount = 0;
};

// ---------- 消息转发 ----------
// 按转发界面配置的规则执行：来源会话收到的数据原样发往目标会话（文本透传，跨串口/网络均可）。
// 只在收到数据（RX）时触发；目标会话必须已连接。转发本身不会再次引发转发，天然无环路。
export const forwardIfConfigured = (session: ConnectionSession, text: string) => {
  for (const rule of forwardRules.value) {
    if (rule.fromId !== session.id) continue;
    const target = sessions.value.find((item) => item.id === rule.toId);
    if (target && target.id !== session.id && target.status === 'connected') {
      void sendData(target, text);
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

// ---------- 参数修改 ----------
// ---------- 自动发送（后端定时线程） ----------
// 勾选「自动」后由 Rust 侧专一线程按间隔直接写连接，绕过前端 IPC——
// 高频发送（如 10ms）不再造成 IPC 堆积与 UI 卡顿。
// 锁定“开启时的会话”，切换视图/会话不会改变目标，杜绝误发。
let loopSessionId: string | undefined;
let sendTextSyncTimer: ReturnType<typeof setTimeout> | undefined;

const stopLoop = async () => {
  if (loopSessionId) {
    const target = sessions.value.find((item) => item.id === loopSessionId);
    if (target) {
      try {
        if (target.type === 'serial') {
          await invoke('serial_auto_send_stop', { port: target.config.port });
        } else {
          await invoke('net_auto_send_stop', { key: target.id });
        }
      } catch {
        /* 会话可能已关闭，忽略 */
      }
    }
    loopSessionId = undefined;
  }
};

const startLoop = () => {
  void (async () => {
    await stopLoop();
    const session = activeSession.value;
    if (!session || session.status !== 'connected' || !session.sendText) {
      sendSettings.value.loopSend = false;
      return;
    }
    loopSessionId = session.id;
    try {
      if (session.type === 'serial') {
        await invoke('serial_auto_send_start', {
          port: session.config.port,
          data: session.sendText + newlineSeq.value,
          intervalMs: Math.max(10, sendSettings.value.loopInterval || 1000),
        });
      } else {
        await invoke('net_auto_send_start', {
          key: session.id,
          data: session.sendText + newlineSeq.value,
          intervalMs: Math.max(10, sendSettings.value.loopInterval || 1000),
        });
      }
    } catch (e) {
      session.statusMsg = `自动发送启动失败: ${e}`;
      sendSettings.value.loopSend = false;
    }
  })();
};

// 发送框内容变化时（防抖 300ms）同步到后端定时线程，自动发送始终发最新内容。
// 注意：watch 源需覆盖所有会话的 sendText，确保 loopSessionId 赋值前就已建立依赖。
watch(
  () => sessions.value.map((item) => item.sendText).join('\u0000'),
  () => {
    if (!sendSettings.value.loopSend || !loopSessionId) return;
    if (sendTextSyncTimer) clearTimeout(sendTextSyncTimer);
    sendTextSyncTimer = setTimeout(() => startLoop(), 300);
  }
);

watch(
  () => sendSettings.value.loopSend,
  (on) => (on ? startLoop() : void stopLoop())
);

watch(
  () => sendSettings.value.loopInterval,
  () => {
    if (sendSettings.value.loopSend) startLoop();
  }
);

// ---------- 生命周期 ----------
const onSystemThemeChange = (e: MediaQueryListEvent) => {
  systemDark.value = e.matches;
};

// ---------- 接收合帧 ----------
// 高波特率下 RX 事件频率可达每秒数百次，逐条渲染会造成卡顿；
// 按会话累积文本，16ms（一帧）批量刷入消息流——显示粒度极限，转发也随之整批进行。
// raw 为原始数据（用于转发透传），prefix 为显示前缀（如 TCP 来源地址），二者分离保证转发不带显示标记。
const pendingRx = new Map<string, { session: ConnectionSession; raw: string; prefix: string }>();
let rxFlushTimer: ReturnType<typeof setTimeout> | undefined;

const flushPendingRx = () => {
  rxFlushTimer = undefined;
  for (const { session, raw, prefix } of pendingRx.values()) {
    const display = prefix + raw;
    session.messageCount += 1;
    appendGlobalMessage(session, 'RX', display);
    appendSessionMessage(session, 'RX', display);
    forwardIfConfigured(session, raw);
  }
  pendingRx.clear();
};

const enqueueRx = (session: ConnectionSession, raw: string, prefix = '') => {
  const item = pendingRx.get(session.id);
  if (item) {
    item.raw += raw;
  } else {
    pendingRx.set(session.id, { session, raw, prefix });
  }
  if (!rxFlushTimer) rxFlushTimer = setTimeout(flushPendingRx, 16);
};

// TX 合帧：自动发送由 Rust 线程直接写出（绕过前端 sendData），通过 auto-sent 事件回填 TX 记录
const pendingTx = new Map<string, { session: ConnectionSession; raw: string }>();
let txFlushTimer: ReturnType<typeof setTimeout> | undefined;

const flushPendingTx = () => {
  txFlushTimer = undefined;
  for (const { session, raw } of pendingTx.values()) {
    session.messageCount += 1;
    appendGlobalMessage(session, 'TX', raw);
    appendSessionMessage(session, 'TX', raw);
  }
  pendingTx.clear();
};

const enqueueTx = (session: ConnectionSession, raw: string) => {
  const item = pendingTx.get(session.id);
  if (item) {
    item.raw += raw;
  } else {
    pendingTx.set(session.id, { session, raw });
  }
  if (!txFlushTimer) txFlushTimer = setTimeout(flushPendingTx, 16);
};

const findSerialSessionByPort = (port: string) =>
  sessions.value.find((item) => item.type === 'serial' && item.config.port === port);

// 装配事件监听与定时器，App.vue 在 onMounted 调用一次。
export const initApp = async () => {
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
    const text = decodeBytes(event.payload.data);
    enqueueRx(session, text);
  });

  unlistenSerialDisconnect = await listen<string>('serial-disconnect', (event) => {
    const session = findSerialSessionByPort(event.payload);
    if (!session) return;

    session.status = 'closed';
    session.statusMsg = `串口 ${event.payload} 已断开`;
    // 若自动发送作用于该会话，断开后弹回开关（后端任务已随 close 停止）
    if (loopSessionId === session.id) {
      sendSettings.value.loopSend = false;
    }
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
    enqueueTx(session, decodeBytes(event.payload.data));
  });

  // auto-send-stopped：自动发送线程写失败退出时上报——弹回开关并提示原因
  unlistenAutoSendStopped = await listen<{ key: string; data: number[] }>('auto-send-stopped', (event) => {
    const key = event.payload.key;
    const session =
      sessions.value.find((item) => item.id === key) ||
      sessions.value.find((item) => item.type === 'serial' && item.config.port === key);
    if (!session) return;
    if (loopSessionId === session.id) {
      sendSettings.value.loopSend = false;
    }
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
    const text = decodeBytes(event.payload.data);
    const prefix = event.payload.from ? `[${event.payload.from}] ` : '';
    enqueueRx(session, text, prefix);
  });

  unlistenNetDisconnect = await listen<string>('net-disconnect', (event) => {
    const session = sessions.value.find((item) => item.id === event.payload);
    if (!session) return;

    session.status = 'closed';
    session.statusMsg = `连接已断开（${session.name}）`;
    // 若自动发送作用于该会话，断开后弹回开关
    if (loopSessionId === session.id) {
      sendSettings.value.loopSend = false;
    }
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
  void stopLoop();
  // 刷掉尚未落盘的合帧缓冲，保证最后一批数据不丢
  if (rxFlushTimer) {
    clearTimeout(rxFlushTimer);
    rxFlushTimer = undefined;
  }
  flushPendingRx();
  // 刷掉尚未落盘的 TX 合帧缓冲
  if (txFlushTimer) {
    clearTimeout(txFlushTimer);
    txFlushTimer = undefined;
  }
  flushPendingTx();
  unlistenSerialData?.();
  unlistenSerialDisconnect?.();
  unlistenNetData?.();
  unlistenNetDisconnect?.();
  unlistenAutoSent?.();
  unlistenAutoSendStopped?.();
};
