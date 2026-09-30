import type { ConnectionSession, ConnectionType, NetConfig } from '../types';

// 会话相关的展示与描述辅助函数，组件与日志模块共用。

// 网络会话的名称：协议类型 + 地址，直观区分 Client/Server
export const netSessionName = (type: ConnectionType, net: NetConfig) => {
  if (type === 'tcp_client') return `TCP Client ${net.host}:${net.port}`;
  if (type === 'tcp_server') return `TCP Server :${net.port}`;
  return `UDP :${net.localPort || net.port}`;
};

// 会话卡片的连接路径描述（标题行下一行，格式 ip:port->ip:port）：
// UDP/双方均已知的 TCP 客户端显示完整路径；TCP 服务端显示监听地址；串口显示端口与波特率
export const sessionSubLabel = (session: ConnectionSession) => {
  if (session.type === 'serial') return `串口 · ${session.config.baudRate}bps`;
  const net = session.net;
  if (!net) return '';
  const local = `${net.localHost || '0.0.0.0'}:${net.localPort || 0}`;
  const remote = `${net.host || '0.0.0.0'}:${net.port || 0}`;
  if (session.type === 'udp') return `${local}->${remote}`;
  if (session.type === 'tcp_client') return net.localPort > 0 ? `${local}->${remote}` : remote;
  return local; // tcp_server：监听地址
};
