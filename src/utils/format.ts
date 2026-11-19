// 时间与字节格式化工具，日志、消息流、总线共用。
// 手动拼 HH:mm:ss.SSS：toLocaleTimeString 走国际化格式化（每次数微秒级），
// 而合帧 flush 里每条消息都要取一次时间，高频下会成为帧内可观测的开销；
// 同时只取一个 Date，避免跨秒时“秒”与“毫秒”来自两个不同时刻。
const pad2 = (n: number) => (n < 10 ? '0' + n : String(n));

// 由毫秒时间戳格式化（nowText 与“单调时间戳”共用，保证格式完全一致）
export const timeText = (ms: number) => {
  const d = new Date(ms);
  return `${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}.${String(
    d.getMilliseconds()
  ).padStart(3, '0')}`;
};

export const nowText = () => timeText(Date.now());

export const todayText = () => {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
};

export const decodeBytes = (data: number[]) => new TextDecoder().decode(new Uint8Array(data));

// 字节数组转十六进制字符串（大写两位、空格分隔），十六进制显示模式用
export const bytesToHex = (data: number[]) =>
  data.map((b) => b.toString(16).padStart(2, '0').toUpperCase()).join(' ');

// HEX 字符串转字节数组：忽略非十六进制字符，按两位解析。
// 奇数长度时在末尾补 0（即 "ABC" → AB C0，按“高位在前”的字节序处理），与发送框实时过滤配合。
export const hexToBytes = (hex: string): number[] => {
  const clean = hex.replace(/[^0-9a-fA-F]/g, '');
  const pairs = clean.length % 2 === 0 ? clean : clean + '0';
  const out: number[] = [];
  for (let i = 0; i < pairs.length; i += 2) out.push(parseInt(pairs.slice(i, i + 2), 16));
  return out;
};
