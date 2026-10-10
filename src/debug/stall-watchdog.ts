// 主线程卡死看门狗（仅 dev 构建启用）。
// 机制：主线程每 200ms 向 Worker 推心跳 + 最近面包屑；Worker 独立线程每 400ms 检查，
// 超过 1.5s 无心跳即在【Worker 自己的控制台】反复输出卡死前的最后操作序列——
// 主线程冻结期间 devtools 的 Worker 上下文仍然可用。
// 用法：卡死后 F12 → Console → 上下文切换到 blob Worker → 查看 [STALL] 条目。
let seq = 0;
const RING: string[] = [];
let worker: Worker | null = null;
let lastBeat = 0;

export const crumb = (msg: string) => {
  seq++;
  RING.push(`#${seq} ${new Date().toISOString().slice(11, 23)} ${msg}`);
  if (RING.length > 400) RING.shift();
  heartbeat();
};

const heartbeat = () => {
  lastBeat = Date.now();
  worker?.postMessage({ t: lastBeat, crumbs: RING.slice(-40) });
};

export const startStallWatchdog = () => {
  if (worker || !import.meta.env.DEV) return;
  const src = `let last=null;self.onmessage=(e)=>{last=Date.now();if(e.data.crumbs)self.__c=e.data.crumbs;};setInterval(()=>{if(last&&Date.now()-last>1500){console.error('[STALL] 主线程已冻结>1.5s，卡死前最后操作：',JSON.stringify(self.__c||[],null,1));}},400);`;
  worker = new Worker(URL.createObjectURL(new Blob([src], { type: "text/javascript" })));
  heartbeat();
  setInterval(heartbeat, 200);
};
