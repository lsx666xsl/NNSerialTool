// 时间与字节格式化工具，日志、消息流、总线共用。
export const nowText = () =>
  new Date().toLocaleTimeString('zh-CN', { hour12: false }) +
  `.${String(new Date().getMilliseconds()).padStart(3, '0')}`;

export const todayText = () => {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
};

export const decodeBytes = (data: number[]) => new TextDecoder().decode(new Uint8Array(data));

// 字节数组转十六进制字符串（大写两位、空格分隔），十六进制显示模式用
export const bytesToHex = (data: number[]) =>
  data.map((b) => b.toString(16).padStart(2, '0').toUpperCase()).join(' ');
