<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import type { SessionMessage } from '../types';
import { autoScroll, fontSize } from '../stores/appStore';
import { bytesToHex } from '../utils/format';
import { splitLogTags, type LogSeg } from '../utils/logLevel';

// 流式消息区：详情视图与分屏视图共用。
// 时间戳与 RX/TX 开关只控制"前缀标签"的显隐——数据本身始终显示。
// 自动滚动（默认关闭，工具栏手动勾选）：勾选后只要有新消息就滚到最底下；
// “↓ 最新”按钮仅在「内容溢出出现滚动条 且 不在底部」时动态出现。
// 支持 Ctrl+滚轮实时调整字号（11-22）。
const props = defineProps<{
  messages: SessionMessage[];
  showTimestamp: boolean;
  filterRx: boolean;
  filterTx: boolean;
  hexMode: boolean;
}>();

// 方向过滤（入队时打标，不补显示）：RX/TX 关闭期间收到的行整行隐藏（数据照常接收与计数，
// 只是永不入框）；重开只显示之后的新数据。
const visibleMessages = computed(() => props.messages.filter((m) => !m.hidden));

// 每条消息生效的显示开关：优先用入队时刻的快照（开关只影响新数据，
// 旧消息保持原样——切 HEX/时间戳不再全量翻转历史）；旧消息（无快照）回退 props。
const msgTs = (m: SessionMessage) => m.snap ? m.snap.ts : props.showTimestamp;
const msgRx = (m: SessionMessage) => m.snap ? m.snap.rx : props.filterRx;
const msgTx = (m: SessionMessage) => m.snap ? m.snap.tx : props.filterTx;
const msgHex = (m: SessionMessage) => m.snap ? m.snap.hex : props.hexMode;

// 方向前缀是否显示（RX/TX 各自独立开关，按消息快照）
const showDirTag = (m: SessionMessage) =>
  m.direction === 'RX' ? msgRx(m) : msgTx(m);

// 纯连续流模式判定也按"当前开关"（决定整体行结构——行式 or 行内连续流）。
// 旧消息混排时以当前开关决定布局（行结构属于容器，不属于单条消息）。
const isContinuous = computed(() => !props.showTimestamp && !props.filterRx && !props.filterTx);

// 单条消息的显示文本：HEX 模式渲染字节流（连续流模式补尾随空格保证块间分隔）
const renderText = (m: SessionMessage) => {
  if (msgHex(m) && m.raw) {
    const hex = bytesToHex(m.raw);
    return isContinuous.value ? hex + ' ' : hex;
  }
  return m.text;
};

// 日志等级着色：文本模式下行首 [INFO]--/[ERROR]-- 等标签切出来染成徽标
// （与固件 NNPrintf.h 的 [LEVEL]-- 前缀格式对齐）；HEX 模式/无标签返回 null 走纯文本。
// v-memo 已保证只在新消息或开关切换时重算，无需缓存。
const logSegmentsOf = (m: SessionMessage): LogSeg[] | null => {
  if (msgHex(m) && m.raw) return null;
  return splitLogTags(m.text);
};

const flowEl = ref<HTMLElement | null>(null);
// "↓ 最新"按钮带滞回：距底 >60px 出现、<4px 才消失，
// 避免临界距离反复横跳导致的按钮闪烁（接收抖动的来源之一）
const showJump = ref(false);

let scrollRaf = 0;

// 刷新滚动状态（滞回判定）
const updateScrollState = () => {
  scrollRaf = 0;
  const el = flowEl.value;
  if (!el) return;
  const dist = el.scrollHeight - el.scrollTop - el.clientHeight;
  showJump.value = el.scrollHeight > el.clientHeight + 2 && (dist > 60 || (showJump.value && dist > 4));
};

const onScroll = () => {
  if (!scrollRaf) scrollRaf = requestAnimationFrame(updateScrollState);
};

const scrollToBottom = () => {
  const el = flowEl.value;
  if (!el) return;
  el.scrollTop = el.scrollHeight;
  updateScrollState();
};

// Ctrl+滚轮：以 1px 步进缩放消息字号
const onWheel = (e: WheelEvent) => {
  if (!e.ctrlKey) return;
  e.preventDefault();
  const next = fontSize.value + (e.deltaY < 0 ? 1 : -1);
  fontSize.value = Math.min(22, Math.max(11, next));
};

// 勾选自动滚动：只要有新消息就滚到最底下（无条件跟随）；同时刷新溢出状态。
// flush:'post'：与本批 DOM 更新同一帧内完成滚动写入——此前的 nextTick+rAF 两级
// 延迟会让滚动落在"下一批补丁中途"的容器高度上，高频流（TCP 每秒 60+ 批）时
// 底部边缘持续 ±数行抖动。
watch(
  () => visibleMessages.value[visibleMessages.value.length - 1]?.id,
  () => {
    const el = flowEl.value;
    if (!el) return;
    if (autoScroll.value) el.scrollTop = el.scrollHeight;
    updateScrollState();
  },
  { flush: 'post' }
);

// 容器尺寸变化（窗口缩放/分屏布局切换）时同步溢出状态
let resizeObserver: ResizeObserver | undefined;
onMounted(() => {
  resizeObserver = new ResizeObserver(() => updateScrollState());
  if (flowEl.value) resizeObserver.observe(flowEl.value);
});
onUnmounted(() => resizeObserver?.disconnect());
</script>

<template>
  <div class="flow-wrap">
    <div
      ref="flowEl"
      class="message-flow"
      :class="{ anchored: visibleMessages.length > 0 && !isContinuous, continuous: isContinuous }"
      :style="{ fontSize: fontSize + 'px' }"
      @scroll="onScroll"
      @wheel="onWheel"
    >
      <div v-if="visibleMessages.length === 0" class="flow-empty">等待接收数据</div>
      <!-- 两行式布局：第一行元信息（时间戳/方向），第二行起为数据正文；时间戳与 RX/TX 独立 -->
      <!-- v-memo 依赖各消息自己的快照字段：开关切换只让"无快照的旧消息"重算，
           带快照的消息按入队时规则固定不变（历史不翻转、不重渲染） -->
      <div
        v-for="message in visibleMessages"
        :key="message.id"
        v-memo="[msgTs(message), msgRx(message), msgTx(message), msgHex(message)]"
        class="flow-line"
        :class="message.direction.toLowerCase()"
      >
        <div class="flow-meta">
          <span v-if="showTimestamp && msgTs(message) && message.time" class="flow-time">{{ message.time }}</span>
          <span v-if="showDirTag(message)" class="flow-dir">{{ message.direction }}</span>
        </div>
        <!-- 十六进制模式：显示原始字节 HEX 流（无原始字节的旧消息回退文本）；
             文本模式：行首 [LEVEL]-- 等级标签染成彩色徽标（双主题显眼） -->
        <div class="flow-text">
          <template v-if="logSegmentsOf(message)">
            <span
              v-for="(seg, i) in logSegmentsOf(message)"
              :key="i"
              :class="seg.cls ? ['lv-tag', 'lv-' + seg.cls] : undefined"
            >{{ seg.text }}</span>
          </template>
          <template v-else>{{ renderText(message) }}</template>
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
  display: flex;
  flex-direction: column;
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
}

/* 有消息时底部锚定：内容不足一屏时贴底显示（终端式），内容溢出时该边距自动归零、正常滚动 */
.message-flow.anchored::before {
  content: '';
  margin-top: auto;
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

.flow-line {
  display: flex;
  flex-direction: column;
  gap: 1px;
  padding: 2px 4px;
  border-radius: 3px;
  white-space: pre-wrap;
  word-break: break-all;
  line-height: 1.5;
  color: #23262b;
}

.flow-line:hover {
  background: rgba(23, 26, 33, 0.04);
}

/* 元信息行：时间戳 + 方向标签，小一号浅色 */
.flow-meta {
  display: flex;
  gap: 8px;
  font-size: 0.85em;
}

.flow-time {
  color: #7c828c;
  font-weight: 400;
}

.flow-dir {
  font-weight: 700;
}

.flow-line.rx .flow-dir {
  color: #2e8b45;
}

.flow-line.tx .flow-dir {
  color: #2b6cb0;
}

/* 日志等级徽标：行首 [INFO]/[ERROR] 等标签着色（浅色主题，与 RX/TX 方向色同族） */
.lv-tag {
  font-weight: 600;
  padding: 0 3px;
  border-radius: 3px;
}

.lv-trace {
  color: #8a9099;
  background: rgba(138, 144, 153, 0.14);
}

.lv-debug {
  color: #2b6cb0;
  background: rgba(43, 108, 176, 0.12);
}

.lv-info {
  color: #2e8b45;
  background: rgba(46, 139, 69, 0.12);
}

.lv-warn {
  color: #b7791f;
  background: rgba(183, 121, 31, 0.14);
}

.lv-error {
  color: #c53030;
  background: rgba(197, 48, 48, 0.12);
}

/* FATAL 最醒目：实心色块反白 */
.lv-fatal {
  color: #ffffff;
  background: #c53030;
}

/* 纯连续流模式（时间戳+RX+TX 全关）：去掉行结构，块转行内，
   数据按到达顺序连成一段（数据内的真实换行符仍会换行） */
.message-flow.continuous {
  display: block;
}

/* 空 meta 块会把 inline 行打断，一并隐藏 */
.message-flow.continuous .flow-meta {
  display: none;
}

.message-flow.continuous .flow-line,
.message-flow.continuous .flow-text {
  display: inline;
  padding: 0;
  border-radius: 0;
}

.message-flow.continuous .flow-line:hover {
  background: transparent;
}

/* 深色主题：深底浅字，方向色提亮 */
.theme-dark .message-flow {
  background: #1a1b1e;
  border-color: rgba(255, 255, 255, 0.09);
  color: #d6d9de;
}

.theme-dark .flow-line {
  color: #d6d9de;
}

.theme-dark .flow-line:hover {
  background: rgba(255, 255, 255, 0.05);
}

.theme-dark .flow-time {
  color: #7c828c;
}

.theme-dark .flow-line.rx .flow-dir {
  color: #6bc97e;
}

.theme-dark .flow-line.tx .flow-dir {
  color: #6ca7e8;
}

/* 深色主题等级徽标：提亮一档 */
.theme-dark .lv-trace {
  color: #6f737a;
  background: rgba(255, 255, 255, 0.06);
}

.theme-dark .lv-debug {
  color: #6ca7e8;
  background: rgba(108, 167, 232, 0.14);
}

.theme-dark .lv-info {
  color: #6bc97e;
  background: rgba(107, 201, 126, 0.14);
}

.theme-dark .lv-warn {
  color: #f6c453;
  background: rgba(246, 196, 83, 0.14);
}

.theme-dark .lv-error {
  color: #f87171;
  background: rgba(248, 113, 113, 0.16);
}

.theme-dark .lv-fatal {
  color: #1a1b1e;
  background: #f87171;
}

.theme-dark .flow-empty {
  color: #6f737a;
}
</style>
