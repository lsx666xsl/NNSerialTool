// 串口基础信息来自 Rust 后端 serial_list_ports 命令，前端只负责展示和选择。
export type SerialPortInfo = {
  portnum: string;
  portproduct: string;
};

// 这几个联合类型相当于“只能从固定值中选择”的枚举，能减少拼写错误。
// TCP/UDP 已在本阶段接入，与串口会话共用同一套会话架构。
export type ConnectionType = 'serial' | 'tcp_client' | 'tcp_server' | 'udp';
export type ConnectionStatus = 'closed' | 'connected';
export type ViewMode = 'detail' | 'split' | 'global' | 'forward';
export type ThemeMode = 'light' | 'dark' | 'system';
export type AppliedTheme = 'light' | 'dark';
// RX/TX 为数据方向；系统提示类信息不再作为消息方向存在（走中央 toast 通知）
export type MessageDirection = 'RX' | 'TX';

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
// raw 为原始字节（十六进制显示模式用；发送侧为实际写出的 UTF-8 字节）
export type SessionMessage = {
  id: string;
  time: string;
  direction: MessageDirection;
  text: string;
  raw?: number[];
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
  // 是否已加入总览：只有加入的会话，收发记录才会汇入总线时间线（选择性加入）
  inBus?: boolean;
  // 是否已提示过“消息达上限被裁剪”，避免重复刷屏
  warnedOverflow?: boolean;
  // 会话级显示开关：时间戳按会话控制；RX/TX 为独立的方向过滤开关（只看收/只看发/都看）
  showTimestamp?: boolean;
  filterRx?: boolean;
  filterTx?: boolean;
  // 十六进制显示模式：亮起后收发数据以 HEX 字节流显示（默认字符串）
  hexMode?: boolean;
  // 发送框十六进制模式：输入内容按 HEX 解析后以原始字节发送（默认字符串模式）
  sendHexMode?: boolean;
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

// 发送相关的可折叠选项：换行符 / 循环发送。
export type SendSettings = {
  newline: 'none' | 'lf' | 'crlf' | 'cr';
  loopSend: boolean;
  loopInterval: number;
};

// 快捷命令按钮：名称 + 内容，点击即发送（参考 VOFA+ / 串口调试助手）。
export type QuickCommand = {
  name: string;
  text: string;
};

// 转发规则：来源会话收到的数据原样发往目标会话（仅限连接列表中已有的会话）
export type ForwardRule = {
  id: string;
  fromId: string;
  toId: string;
};
