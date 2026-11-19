<script setup lang="ts">
import { computed, nextTick, onActivated, onMounted, onUnmounted, ref, shallowRef, watch } from 'vue';
import type { SessionMessage } from '../types';
import { fontSize, showMessageMeta } from '../stores/appStore';
import MessageChunk from './MessageChunk.vue';

// 流式消息区：详情视图与分屏视图共用。
// 时间戳与 RX/TX 是"前缀标签"，仅作显示；工具栏的「标签」总开关可一次性隐藏全部标签
//（不影响接收、方向过滤与消息内容）。自动滚动是每个会话自己的开关（由父组件传入），
// 勾选后只要有新消息就滚到最底下；“↓ 最新”按钮仅在「内容溢出出现滚动条 且 不在底部」时动态出现。
// 支持 Ctrl+滚轮实时调整字号（11-22）。
const props = defineProps<{
  messages: SessionMessage[];
  autoScroll: boolean;
}>();

// 点「↓ 最新」= 用户表达“回到最新并继续跟随”，由父组件把该会话的自动滚动重新打开
const emit = defineEmits<{ (e: 'follow'): void }>();

// 方向过滤（入队时打标，不补显示）：RX/TX 关闭期间收到的行整行隐藏（数据照常接收与计数，
// 只是永不入框）；重开只显示之后的新数据。
// 维护方式：不每帧对整表 filter（10 万行时那是 O(n) 的固定开销），
// 而是增量追加；结构变化（清空/裁剪/换会话）才整体重建。
// 用 shallowRef 保存数组本身：赋值即触发依赖，避免“改了普通数组但视图不知道”的隐蔽失同步。
const visibleRows = shallowRef<SessionMessage[]>([]);
let syncedCount = 0;
let syncedHead = '';
let lastMessageArray: SessionMessage[] | undefined;
// 可见行数组的内容版本号：原地追加后靠它触发窗口重算（可见数组本身不再是“每帧新对象”）
const rowsVersion = ref(0);

const syncVisible = () => {
  const all = props.messages;
  // 数组身份变化 = 切换会话（即使新旧消息数相同也必须重建），不能只比较 length
  if (all !== lastMessageArray) {
    lastMessageArray = all;
    syncedCount = 0;
    syncedHead = '';
    visibleRows.value = [];
    rowsVersion.value++;
  }
  const rebuilt = all.length < syncedCount || (syncedCount > 0 && all[0]?.id !== syncedHead);
  if (rebuilt) {
    const next: SessionMessage[] = [];
    for (let i = 0; i < all.length; i++) if (!all[i].hidden) next.push(all[i]);
    syncedCount = all.length;
    syncedHead = all[0]?.id ?? '';
    visibleRows.value = next;
    rowsVersion.value++;
    return;
  }
  if (all.length === syncedCount) {
    // 长度没变但内容可能被整体替换（例如清空后又立刻收到同样条数）：靠上面的身份检查兜底
    return;
  }
  // 追加：**原地 push + 版本号**，不再整数组 slice() 复制。
  // 之前每帧复制整条可见数组（10 万行时每帧复制 10 万引用）→ 数组身份每帧都变，
  // 窗口整体重渲染 + GC 抖动，视觉上就是“闪”。
  const cur = visibleRows.value;
  for (let i = syncedCount; i < all.length; i++) if (!all[i].hidden) cur.push(all[i]);
  syncedCount = all.length;
  rowsVersion.value++;
};

// 按帧同步：数组身份变化（切换会话）或长度变化都触发，等长切换不会复用旧数据
watch([() => props.messages, () => props.messages.length], syncVisible, { flush: 'pre', immediate: true });

const isEmpty = computed(() => {
  rowsVersion.value;
  return visibleRows.value.length === 0;
});

// 少量消息时保持原来的终端式“贴底”观感：窗口显示在可视区底部；
// 超过一屏后交给正常滚动位置控制，不影响虚拟索引
const bottomGap = computed(() => {
  version.value;
  viewH.value;
  scrollTop.value;
  return scrollTop.value === 0 ? Math.max(0, viewH.value - totalHeight()) : 0;
});

// ==================================================================
// 虚拟滚动：只渲染视口内的若干行，其余用占位高度撑开。
// 为什么必须这样做：实测同一份数据、只把 DOM 行数从 9000 减到 700，单条刷新延迟
// 24.8ms → 15.9ms——浏览器端“追加一行”的成本随已有 DOM 行数增长（与 flex/block 布局无关）。
// 行高不定（日志会折行），因此按行记录实测高度，用「分块前缀和」把
// “滚动位置 ↔ 行下标”的换算压到 O(总行数/128 + 128)，10 万行也不随行数变慢。
// ==================================================================
const BLOCK = 128; // 前缀和分块大小（2 的幂，便于用移位取块号）
// 未测量行的估计高度：由字号 + 是否显示标签推导。实测两套线性关系（误差 ≤1px）：
//   标签显示（时间戳 + 方向/前缀两行）：12px→38、14px→44、16px→49、18px→55  ⇒ 3.05*字号+1.3
//   只有正文（标签隐藏）        ：12px→22、14px→25、16px→28、18px→31  ⇒ 1.5*字号+4
// 硬编码单一常数会让首遍扫描低估 10%~25%（标签关时更严重），之后由中位数样本继续修正。
const estimateFor = (fontPx: number, withMeta: boolean) =>
  withMeta ? Math.round(3.05 * fontPx + 1.3) : Math.round(1.5 * fontPx + 4);
const OVERSCAN = 6; // 视口上下各多渲染几行，避免快速滚动出现空白

let hArr = new Float64Array(4096); // 每行实测高度；0 = 未测量（按 estH 计）
let estH = estimateFor(14, true); // 当前估计行高（初始按默认字号 14px + 标签显示）
let blkSum = new Float64Array(32); // 每块的高度和（与 heightAt 严格一致）
let blkMeasured = new Uint32Array(32); // 每块中“已测量”的行数（用于估计值调整）
// 行高样本环，估计值取**中位数**：不用均值——像“一次发 3000 行”生成的巨型 TX 行这种离群值
// 会把均值带飞，并形成“估算↑→未测行虚高→均值↑”的正反馈（实测总高度会涨到 8 倍）。
const SAMPLE_MAX = 64;
const sampleBuf = new Float64Array(SAMPLE_MAX);
let sampleCount = 0;
let samplePos = 0;
let nRows = 0; // 已登记行数（与 visibleRows 对齐）
let headId = ''; // 索引对应的首行 id：首行变了（裁剪/换会话）就整体重建
let measureQueued = false; // 渲染后测量只排队一次，避免 watch 回调堆积

// 块 b 的行数（末块可能不满）
const blockCount = (b: number) => Math.min(BLOCK, Math.max(0, nRows - (b << 7)));

const ensureCap = (n: number) => {
  if (hArr.length >= n) return;
  let cap = hArr.length;
  while (cap < n) cap *= 2;
  const nextH = new Float64Array(cap);
  nextH.set(hArr);
  hArr = nextH;
  const nextB = new Float64Array(Math.ceil(cap / BLOCK) + 1);
  nextB.set(blkSum);
  blkSum = nextB;
  const nextM = new Uint32Array(Math.ceil(cap / BLOCK) + 1);
  nextM.set(blkMeasured);
  blkMeasured = nextM;
};

const heightAt = (i: number) => hArr[i] || estH;

// 索引与列表结构对齐：只追加（最常见）、结构变化（裁剪/换会话）则整体重建
const syncIndex = () => {
  const rows = visibleRows.value;
  const rebuilt = rows.length < nRows || (nRows > 0 && rows[0]?.id !== headId);
  if (rebuilt) {
    nRows = 0;
    sampleCount = 0;
    samplePos = 0;
    blkMeasured.fill(0);
  }
  if (rows.length !== nRows) {
    ensureCap(rows.length);
    if (rebuilt) blkSum.fill(0);
    for (let i = nRows; i < rows.length; i++) {
      hArr[i] = 0;
      blkSum[i >> 7] += estH;
    }
    nRows = rows.length;
    rebuildTree();
  }
  headId = rows[0]?.id ?? '';
};

// 估计值滚动更新：取已测样本的**中位数**，并把变化量只摊到“未测量行”上（O(块数)）。
// 用中位数而不是均值，避免巨型行（一次发几千行）把估计值带偏。
const rebalanceEstimate = () => {
  if (sampleCount < 8) return false; // 样本太少不调，避免被最初几行带偏
  const sorted = Array.from(sampleBuf.subarray(0, sampleCount)).sort((a, b) => a - b);
  const median = sorted[Math.floor(sorted.length / 2)];
  const next = Math.min(400, Math.max(8, Math.round(median)));
  const delta = next - estH;
  if (Math.abs(delta) < 2) return false;
  estH = next;
  const blocks = (nRows >> 7) + 1;
  for (let b = 0; b < blocks; b++) {
    const unmeasured = blockCount(b) - blkMeasured[b];
    if (unmeasured > 0) blkSum[b] += delta * unmeasured;
  }
  rebuildTree();
  return true;
};

// 行 i 之前的高度和。块前缀使用 Fenwick 树，更新/查询 O(log 块数）；
// 否则 10 万行约 782 块，滚动时每帧重复线性求和会重新引入 O(n) 成本。
let tree = new Float64Array(32);
const treeAdd = (index: number, delta: number) => {
  for (let i = index + 1; i < tree.length; i += i & -i) tree[i] += delta;
};
const treePrefix = (count: number) => {
  let sum = 0;
  for (let i = count; i > 0; i -= i & -i) sum += tree[i];
  return sum;
};
const rebuildTree = () => {
  tree = new Float64Array(blkSum.length);
  for (let b = 0; b < blkSum.length; b++) treeAdd(b, blkSum[b]);
};
const prefixBlock = (block: number) => treePrefix(Math.min(block, blkSum.length));
const prefixTo = (i: number) => {
  const block = i >> 7;
  let sum = prefixBlock(block);
  for (let k = block << 7; k < i; k++) sum += heightAt(k);
  return sum;
};

// 总高度
const totalHeight = () => treePrefix(nRows >> 7) + (() => {
  let sum = 0;
  for (let i = (nRows >> 7) << 7; i < nRows; i++) sum += heightAt(i);
  return sum;
})();

// 高度坐标 → 行下标（块前缀上二分定位 + 块内线性）
const rowAtY = (y: number) => {
  if (y <= 0) return 0;
  const blocks = (nRows >> 7) + 1;
  // 找第一个“前缀和 > y”的块：lo 之前完整包含，hi 之后包含目标行
  let lo = 0;
  let hi = blocks;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (prefixBlock(mid + 1) <= y) lo = mid + 1;
    else hi = mid;
  }
  let acc = prefixBlock(lo);
  let i = lo << 7;
  const end = Math.min(nRows, (lo + 1) << 7);
  while (i < end && acc + heightAt(i) <= y) {
    acc += heightAt(i);
    i++;
  }
  return Math.min(i, Math.max(0, nRows - 1));
};

// 记录实测行高并同步分块和。估计值的调整统一由 rebalanceEstimate() 在测量批次结束时做，
// 避免“拿单个样本整体重建”带来的偏差（首个样本可能是超长折行行）。
const setHeight = (i: number, value: number) => {
  if (i >= nRows || value <= 0) return false;
  const b = i >> 7;
  const old = hArr[i] || estH;
  // offsetHeight 是整数：不做 0.5px 忽略阈值，避免 10 万行的累计误差
  if (value === old) return false;
  if (!hArr[i]) {
    // 第一次测到这一行：登记块内已测数，并把高度放进中位数样本环
    blkMeasured[b] += 1;
    sampleBuf[samplePos] = value;
    samplePos = (samplePos + 1) % SAMPLE_MAX;
    if (sampleCount < SAMPLE_MAX) sampleCount += 1;
  }
  hArr[i] = value;
  blkSum[b] += value - old;
  treeAdd(b, value - old);
  return true;
};

// ---------- 视口与窗口 ----------
const flowEl = ref<HTMLElement | null>(null);
const windowEl = ref<HTMLElement | null>(null);
const scrollTop = ref(0);
const viewH = ref(600);
const version = ref(0); // 行数或行高变化后触发窗口重算

const range = computed(() => {
  version.value;
  rowsVersion.value; // 原地追加后靠版本号重算窗口
  syncIndex();
  const n = nRows;
  if (n === 0) return { start: 0, end: 0, padTop: 0, total: 0 };
  const top = scrollTop.value - OVERSCAN * estH;
  const bottom = scrollTop.value + viewH.value + OVERSCAN * estH;
  const start = rowAtY(Math.max(0, top));
  const end = Math.min(n, rowAtY(Math.max(0, bottom)) + 1);
  return { start, end, padTop: prefixTo(start), total: totalHeight() };
});

const windowRows = computed(() => {
  rowsVersion.value; // 原地追加也要重算窗口
  return visibleRows.value.slice(range.value.start, range.value.end);
});

// 渲染后测量真实行高：修正占位高度，避免长行折行后滚动位置漂移。
// 高度修正会改变总高度：如果测量前用户就在底部（跟随中，或手动拉到底），测量后必须再贴一次，
// 否则会停在“按旧估算高度算出的底部”，差出十几行（手动滚动路径没有跟随逻辑兜底）。
const measureRows = () => {
  const wrap = windowEl.value;
  if (!wrap || nRows === 0) return;
  const els = wrap.querySelectorAll<HTMLElement>('.flow-line');
  if (els.length === 0) return;
  const el = flowEl.value;
  const wasAtBottom = el ? atBottom(el) : false;
  const start = range.value.start;
  let changed = false;
  for (let k = 0; k < els.length; k++) {
    if (setHeight(start + k, els[k].offsetHeight)) changed = true;
  }
  // 批次结束后按已测均值滚动更新估计值（只影响未测量的行）
  const rebalanced = rebalanceEstimate();
  if (!changed && !rebalanced) return;
  version.value++;
  if (wasAtBottom) {
    const cur = flowEl.value;
    if (cur && cur.scrollHeight - cur.scrollTop - cur.clientHeight > 2) {
      cur.scrollTop = cur.scrollHeight;
      updateScrollState();
    }
  }
};

// ---------- 滚动 / 跟随 ----------
// "↓ 最新"按钮带滞回：距底 >60px 出现、<4px 才消失，
// 避免临界距离反复横跳导致的按钮闪烁（接收抖动的来源之一）
const showJump = ref(false);
let scrollRaf = 0;
// 跟随暂停态：autoScroll 开启时用户向上翻看即暂停跟随（与工具栏 tooltip 的描述一致），
// 滚回底部或点「↓ 最新」后恢复。没有这个状态，用户一上翻就会被新数据拽回底部。
let followPaused = false;
// 程序化写 scrollTop 的窗口期：窗口内的 scroll 事件不算“用户上翻”，
// 否则贴底与高度修正时浏览器回夹滚动位置会被误判成用户滚动，跟随会被永久暂停。
let selfScrollUntil = 0;

const atBottom = (el: HTMLElement) => el.scrollHeight - el.scrollTop - el.clientHeight <= 4;

// 统一出口：写滚动位置并标记“这是我们自己滚的”（下一帧结束前有效）
const setScrollTop = (el: HTMLElement, top: number) => {
  selfScrollUntil = performance.now() + 120;
  el.scrollTop = top;
};

const updateScrollState = () => {
  scrollRaf = 0;
  const el = flowEl.value;
  if (!el) return;
  scrollTop.value = el.scrollTop;
  viewH.value = el.clientHeight;
  if (atBottom(el)) followPaused = false;
  else if (props.autoScroll && performance.now() > selfScrollUntil) {
    // 非程序化滚动且离开了底部 = 用户在翻看历史 → 暂停跟随
    followPaused = true;
  }
  const dist = el.scrollHeight - el.scrollTop - el.clientHeight;
  showJump.value = el.scrollHeight > el.clientHeight + 2 && (dist > 60 || (showJump.value && dist > 4));
};

const onScroll = () => {
  if (!scrollRaf) scrollRaf = requestAnimationFrame(updateScrollState);
  queueMeasure(); // 滚动窗口变化后也需要测量新进入视口的行
};

// 贴底：估算行高会让 scrollHeight 偏小，测量修正后需要再校正一次，
// 否则「↓ 最新」会停在离底部几十像素的位置（用户看到最后一条被截断）。
const stickToBottom = (force = false) => {
  const el = flowEl.value;
  if (!el) return;
  const keepGoing = () => force || (props.autoScroll && !followPaused);
  setScrollTop(el, el.scrollHeight);
  updateScrollState();
  queueMeasure();
  // 行高是渲染后逐批实测的：总高度可能连续几帧继续变大，只校正一次会留下
  // 4~60px 的“残差死区”（既回不到底、又不到出现「↓ 最新」的阈值）。连续校正到稳定，最多 3 帧。
  let tries = 0;
  const again = () => {
    const cur = flowEl.value;
    if (!cur || !keepGoing()) return;
    if (cur.scrollHeight - cur.scrollTop - cur.clientHeight > 2 && tries < 3) {
      tries += 1;
      setScrollTop(cur, cur.scrollHeight);
      updateScrollState();
      queueMeasure();
      requestAnimationFrame(again);
    }
  };
  requestAnimationFrame(again);
};

// 用户点「↓ 最新」：清暂停态、回到底部，并请父组件恢复跟随（等价于重新打开自动滚动）
const scrollToBottom = () => {
  followPaused = false;
  emit('follow');
  stickToBottom();
};

// Ctrl+滚轮：以 1px 步进缩放消息字号。
// 普通滚轮不在这里判暂停：滚轮真正滚动时会产生 scroll 事件，由 updateScrollState 统一判断
//（“非程序化滚动且离开底部”才暂停）。这样在内容不足一屏时滚轮不会误暂停跟随。
const onWheel = (e: WheelEvent) => {
  if (!e.ctrlKey) return;
  e.preventDefault();
  const next = fontSize.value + (e.deltaY < 0 ? 1 : -1);
  fontSize.value = Math.min(22, Math.max(11, next));
};

// 新消息到达：列表结构变化 → 重算窗口；自动滚动时贴底。
// flush:'post'：与本批 DOM 更新同一帧内完成滚动写入（与旧实现同款，避免底部抖动）。
watch(
  rowsVersion,
  () => {
    version.value++;
  },
  { flush: 'pre' }
);

watch(
  rowsVersion,
  () => {
    const el = flowEl.value;
    if (!el) return;
    // 跟随时贴底并在测量后校正；非跟随（或用户已上翻暂停）时也要刷新按钮状态，
    // 否则内容增长后「↓ 最新」不会出现，用户无从得知下面还有新数据
    if (props.autoScroll && !followPaused) stickToBottom();
    else updateScrollState();
  },
  { flush: 'post' }
);

// 每次渲染后测量；高度修正会触发下一轮窗口重算（收敛即停止）。
// 统一排队而不是 watch(range)：range 的 computed 对象在滚动时每帧变化，
// 多次 nextTick 回调会在同一批里重复测量，反而放大成本。
const queueMeasure = () => {
  if (measureQueued) return;
  measureQueued = true;
  void nextTick(() => {
    measureQueued = false;
    measureRows();
  });
};

// 字号/标签/宽度变化会让折行高度失效：已测高度全部重置，并按当前锚点重测。
// estH 与“已测行统计”必须一起复位：否则旧字号阶段测出的大行高会继续当作未渲染行的估计值，
// 让 scrollHeight 虚增（滚动条偏长、贴底失效）。
const invalidateHeights = () => {
  estH = estimateFor(fontSize.value, showMessageMeta.value);
  sampleCount = 0;
  samplePos = 0;
  blkMeasured.fill(0);
  if (nRows > 0) hArr.fill(0, 0, nRows);
  for (let b = 0; b < blkSum.length; b++) blkSum[b] = blockCount(b) * estH;
  rebuildTree();
  version.value++;
  queueMeasure();
  // 字号/标签变化后行高整体改变：贴底要走 stickToBottom（多帧校正），
  // 只写一次 scrollTop 会停留在“按旧高度算出的底部”，差出整屏。
  const el = flowEl.value;
  if (el && props.autoScroll && !followPaused) {
    void nextTick(() => {
      queueMeasure();
      stickToBottom();
    });
  }
};
watch([fontSize, showMessageMeta], () => {
  void nextTick(invalidateHeights);
}, { flush: 'post' });

// 容器尺寸变化（窗口缩放/分屏布局切换）时同步视口与溢出状态
let resizeObserver: ResizeObserver | undefined;
let lastWidth = 0;
onMounted(() => {
  const el = flowEl.value;
  if (el) {
    viewH.value = el.clientHeight;
    lastWidth = el.clientWidth;
    resizeObserver = new ResizeObserver(() => {
      viewH.value = el.clientHeight;
      updateScrollState();
      if (el.clientWidth !== lastWidth) {
        lastWidth = el.clientWidth;
        invalidateHeights();
      } else {
        queueMeasure();
      }
    });
    resizeObserver.observe(el);
  }
  queueMeasure();
});
onUnmounted(() => {
  resizeObserver?.disconnect();
  if (scrollRaf) cancelAnimationFrame(scrollRaf);
});

// KeepAlive 切回：缓存期间数据持续入缓冲，等本批 DOM 渲染后再贴底/量行高
onActivated(() => {
  version.value++;
  void nextTick(() => {
    if (props.autoScroll) scrollToBottom();
    else updateScrollState();
    queueMeasure();
  });
});
</script>

<template>
  <div class="flow-wrap">
    <div
      ref="flowEl"
      class="message-flow"
      :style="{ fontSize: fontSize + 'px' }"
      @scroll.passive="onScroll"
      @wheel="onWheel"
    >
      <div v-if="isEmpty" class="flow-empty">等待接收数据</div>
      <!-- 占位层：总高度由行高索引撑开；窗口层只渲染视口附近的行 -->
      <div v-else class="flow-space" :style="{ height: range.total + 'px', marginTop: bottomGap + 'px' }">
        <div
          ref="windowEl"
          class="flow-window"
          :style="{ transform: `translate3d(0, ${Math.round(range.padTop)}px, 0)` }"
        >
          <MessageChunk :rows="windowRows" :hide-meta="!showMessageMeta" />
        </div>
      </div>
    </div>
    <button
      v-if="showJump"
      class="jump-bottom"
      title="滚动到最新数据"
      @click="scrollToBottom"
    >
      ↓ 最新
    </button>
  </div>
</template>

<style scoped>
.flow-wrap {
  position: relative;
  flex: 1;
  min-height: 0;
  display: flex;
}

.jump-bottom {
  position: absolute;
  right: 14px;
  bottom: 14px;
  z-index: 5;
  padding: 5px 12px;
  background: rgba(59, 111, 212, 0.92);
  color: #ffffff;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
}

.message-flow {
  flex: 1;
  min-height: 0;
  overflow: auto;
  /* block 布局：追加行时不必重排全部子项 */
  display: block;
  border: 1px solid rgba(23, 26, 33, 0.1);
  border-radius: 6px;
  /* 跟随主题：浅色浅底深字 */
  background: #f8f9fb;
  color: #23262b;
  padding: 8px;
  font-size: 12px;
  scrollbar-width: thin;
  scrollbar-color: rgba(128, 132, 140, 0.45) transparent;
  /* 滚动条槽位常驻：首次溢出/消失时不再引发整行重排（接收抖动来源之二） */
  scrollbar-gutter: stable;
  /* 关掉浏览器滚动锚定：高度索引修正时浏览器会自己补偿 scrollTop，
     与我们的手动贴底/校正互相抢，表现为“闪一下”。 */
  overflow-anchor: none;
}

/* 占位层：position:relative 让窗口层可以按 padTop 偏移 */
.flow-space {
  position: relative;
}

/* 窗口层：只包含视口附近的行，整体用 transform 定位（不参与布局计算）。
   translate3d + will-change 让浏览器把它放到独立合成层：整层平移不再重绘文字，
   亚像素偏移也已被 Math.round 消掉，避免滚动/追加时的文字抖动与闪烁。 */
.flow-window {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  will-change: transform;
  contain: layout style paint;
  backface-visibility: hidden;
}

.message-flow::-webkit-scrollbar {
  width: 8px;
}

.message-flow::-webkit-scrollbar-thumb {
  background: rgba(128, 132, 140, 0.45);
  border-radius: 4px;
}

.message-flow::-webkit-scrollbar-track {
  background: transparent;
}

.flow-empty {
  color: #6b7280;
  text-align: center;
  padding: 24px 0;
}

/* 深色主题：容器配色（行内配色在 MessageChunk.vue 里） */
.theme-dark .message-flow {
  background: #1a1b1e;
  border-color: rgba(255, 255, 255, 0.09);
  color: #d6d9de;
}

.theme-dark .flow-empty {
  color: #6f737a;
}
</style>





