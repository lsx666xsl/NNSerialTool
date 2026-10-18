// localStorage 小工具：读写失败（隐私模式/序列化异常）时静默回退默认值，不阻塞界面。
export function readStorage<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw === null ? fallback : (JSON.parse(raw) as T);
  } catch {
    return fallback;
  }
}

// 带类型守卫的读取：反序列化结果必须通过 guard 校验，否则回退默认值。
// 存储内容可能来自旧版本或被手工改坏，直接 as 断言会把 "undefined"、非法枚举等带进业务层
// （此前 newline 读到非法值会拼出 "undefined" 发出去）。
export function readStorageChecked<T>(key: string, fallback: T, guard: (value: unknown) => value is T): T {
  try {
    const raw = localStorage.getItem(key);
    if (raw === null) return fallback;
    const parsed: unknown = JSON.parse(raw);
    return guard(parsed) ? parsed : fallback;
  } catch {
    return fallback;
  }
}

// 常用存储值守卫：只保留与业务期望一致的类型，其余一律视为非法
export const isString = (value: unknown): value is string => typeof value === 'string';
export const isBoolean = (value: unknown): value is boolean => typeof value === 'boolean';
export const isFiniteNumber = (value: unknown): value is number =>
  typeof value === 'number' && Number.isFinite(value);

export function writeStorage(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* 忽略写入失败 */
  }
}
