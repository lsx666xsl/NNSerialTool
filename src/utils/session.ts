import type { ConnectionSession, ConnectionType, NetConfig } from '../types';

// 会话相关的展示与描述辅助函数，组件与日志模块共用。

// 网络会话的名称：协议类型 + 地址，直观区分 Client/Server（冒号紧跟类型名）
export const netSessionName = (type: ConnectionType, net: NetConfig) => {
  if (type === 'tcp_client') return `TCP Client ${net.host}:${net.port}`;
  if (type === 'tcp_server') return `TCP Server:${net.port}`;
  return `UDP:${net.localPort || net.port}`;
};

// 会话卡片的连接路径描述（标题行下一行）：
// - 串口：波特率；
// - TCP Server：真实监听地址（host 为空时后端绑定 0.0.0.0，port 是监听端口）；
// - TCP Client：只有目标地址（客户端不 bind 本地端口，不要再显示 localHost/localPort）；
// - UDP：本地绑定 -> 目标（localPort=0 时后端沿用目标端口，与实际绑定一致）。
export const sessionSubLabel = (session: ConnectionSession) => {
  if (session.type === 'serial') return `Baudrate: ${session.config.baudRate}bps`;
  const net = session.net;
  if (!net) return '';
  const remote = `${net.host || '0.0.0.0'}:${net.port || 0}`;
  if (session.type === 'tcp_client') return remote;
  if (session.type === 'tcp_server') return `${net.host || '0.0.0.0'}:${net.port || 0}`;
  const local = `${net.localHost || '0.0.0.0'}:${net.localPort > 0 ? net.localPort : net.port || 0}`;
  return `${local}->${remote}`;
};
