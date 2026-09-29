import type { ConnectionSession, ConnectionType, NetConfig } from '../types';

// 会话相关的展示与描述辅助函数，组件与日志模块共用。

export const sessionTypeLabel = (type: ConnectionType) => {
  const labels: Record<ConnectionType, string> = {
    serial: '串口',
    tcp_client: 'TCP Client',
    tcp_server: 'TCP Server',
    udp: 'UDP',
  };
  return labels[type];
};

// 网络会话的名称：协议类型 + 地址，直观区分 Client/Server
export const netSessionName = (type: ConnectionType, net: NetConfig) => {
  if (type === 'tcp_client') return `TCP Client ${net.host}:${net.port}`;
  if (type === 'tcp_server') return `TCP Server :${net.port}`;
  return `UDP :${net.localPort || net.port}`;
};

// 会话卡片的灰色副标题：纯地址/参数描述，不与标题中的类型重复
export const sessionSubLabel = (session: ConnectionSession) => {
  if (session.type === 'serial') return `串口 · ${session.config.baudRate}bps`;
  if (session.type === 'tcp_server') return `Listen :${session.net?.port ?? ''}`;
  if (session.type === 'udp') return `Local :${session.net?.localPort || session.net?.port || ''}`;
  return `Remote ${session.net?.host}:${session.net?.port}`;
};
