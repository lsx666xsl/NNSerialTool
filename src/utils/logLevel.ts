// 日志等级前缀识别：把 [INFO]-- / [ERROR]-- / ERROR: 这类行首等级标签切出来，
// 供消息区着色显示（与固件 NNPrintf.h 的 [LEVEL]-- 前缀格式对齐）。
// 设计约束：单行/多行文本都要能识别（TCP 流式分片可能把多行拼进一条消息）；
// 只认行首标签（带 [] 或后跟 -- : ：分隔符），避免把正文里的普通单词误染色。

export interface LogSeg {
  text: string;
  cls?: string; // 等级样式类：trace/debug/info/warn/error/fatal，缺省为普通文本
}

const LEVELS = 'TRACE|DEBUG|INFO|WARN|WARNING|ERROR|FATAL';

const LEVEL_CLASS: Record<string, string> = {
  TRACE: 'trace',
  DEBUG: 'debug',
  INFO: 'info',
  WARN: 'warn',
  WARNING: 'warn',
  ERROR: 'error',
  FATAL: 'fatal',
};

// 行首 + 可选缩进 + （[标签] 或 裸标签后跟 -- : ：）；i 支持小写固件，g 全文扫描
const RE = new RegExp(`(^|\\n)[ \\t]*(?:\\[(${LEVELS})\\]|\\b(${LEVELS})\\b(?=--|:|：))`, 'gi');

/**
 * 把文本切成"普通片段 + 等级标签片段"序列；无标签时返回 null（调用方可走纯文本渲染，
 * 与旧路径完全一致，避免高频纯数据流多做一次切片）。
 */
export function splitLogTags(text: string): LogSeg[] | null {
  RE.lastIndex = 0;
  const segs: LogSeg[] = [];
  let prev = 0;
  let found = false;
  for (let m = RE.exec(text); m !== null; m = RE.exec(text)) {
    const tagWithBracket = m[2];
    const tagBare = m[3];
    // 完整标签文本（括号形式含两侧 []），m[0] 尾部这一段才是标签本体
    const tagText = tagWithBracket !== undefined ? `[${tagWithBracket}]` : (tagBare ?? '');
    if (!tagText) continue;
    const cls = LEVEL_CLASS[(tagWithBracket ?? tagBare ?? '').toUpperCase()];
    if (cls === undefined) continue; // 不可能：正则只产出已知等级
    const tagLen = tagText.length;
    const plainHead = text.slice(prev, m.index) + m[0].slice(0, m[0].length - tagLen);
    if (plainHead) segs.push({ text: plainHead });
    segs.push({ text: tagText, cls });
    prev = m.index + m[0].length;
    found = true;
  }
  if (!found) return null;
  if (prev < text.length) segs.push({ text: text.slice(prev) });
  return segs;
}
