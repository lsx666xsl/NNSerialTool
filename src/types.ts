// 串口基础信息来自 Rust 后端 serial_list_ports 命令，前端只负责展示和选择。
export type SerialPortInfo = {
  portnum: string;
  portproduct: string;
};

// 这几个联合类型相当于“只能从固定值中选择”的枚举，能减少拼写错误。
// TCP/UDP 已在本阶段接入，与串口会话共用同一套会话架构。
export type ConnectionType = 'serial' | 'tcp_client' | 'tcp_server' | 'udp';
export type ConnectionStatus = 'closed' | 'connected';
export type ViewMode = 'detail' | 'split' | 'global';
export type ThemeMode = 'light' | 'dark' | 'system';
export type AppliedTheme = 'light' | 'dark';
export type MessageDirection = 'RX' | 'TX' | 'INFO';

// 串口连接参数单独抽出来，方便以后做“场景保存”时直接序列化成配置文件。
export type SerialConfig = {
  port: string;
  baudRate: number;
  dataBits: number;
  parityBits: string;
  stopBits: number;
};

// 网络连接参数：TCP 客户端/UDP 用 host+port 作目标，TCP 服务端用 port 作监听端口，
// UDP 另有 localPort（本地绑定端口，0 表示自动分配）与 localHost（本地绑定地址，空表示 0.0.0.0）。
export type NetConfig = {
  host: string;
  port: number;
  localPort: number;
  localHost: string;
};

// 会话内的一条收发记录：流式渲染的最小单元，time 只在需要时展示。
export type SessionMessage = {
  id: string;
  time: string;
  direction: MessageDirection;
  text: string;
};

// 一个 ConnectionSession 就是一条独立连接，也就是界面左侧列表中的一项。
// 关键点：每个会话自己保存 sendText/messages/status，所以多个连接互不影响。
export type ConnectionSession = {
  id: string;
  name: string;
  type: ConnectionType;
  status: ConnectionStatus;
  config: SerialConfig;
  net?: NetConfig;
  sendText: string;
  messages: SessionMessage[];
  messageCount: number;
  // 会话累计收发字节数：连接期间持续累加（清空消息不影响统计，重开连接不清零）
  txBytes: number;
  rxBytes: number;
  statusMsg: string;
  // 消息转发目标会话 id：本会话收到的数据会原样发往目标会话（空表示不转发）
  forwardTo?: string;
  // 是否已提示过“消息达上限被裁剪”，避免重复刷屏
  warnedOverflow?: boolean;
  // 会话级显示开关：时间戳按会话控制；RX/TX 为独立的方向过滤开关（只看收/只看发/都看）
  showTimestamp?: boolean;
  filterRx?: boolean;
  filterTx?: boolean;
};

// 全局消息总线用于把所有连接的收发记录放到同一个时间线里。
// 它不是替代每个会话的 messages，而是额外提供“跨连接观察顺序”的视角。
export type GlobalMessage = {
  id: string;
  time: string;
  sessionId: string;
  sessionName: string;
  direction: MessageDirection;
  text: string;
};

// 发送相关的可折叠选项：换行符 / 发送后清空 / 循环发送。
export type SendSettings = {
  newline: 'none' | 'lf' | 'crlf' | 'cr';
  clearAfterSend: boolean;
  loopSend: boolean;
  loopInterval: number;
};

// 快捷命令按钮：名称 + 内容，点击即发送（参考 VOFA+ / 串口调试助手）。
export type QuickCommand = {
  name: string;
  text: string;
};
