<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue';
import type { SessionMessage } from '../types';
import { autoScroll, fontSize } from '../stores/appStore';

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
}>();

// 全量显示（不过滤数据）：RX/TX/时间戳开关只作用于前缀标签。
// 注意：不做渲染切片——切片裁剪会让 scrollHeight 突变、干扰自动滚动判定（历史 bug）；
// 上限 2000 条由 store 裁剪，keyed diff + v-memo 让逐条追加的 diff 成本 O(1)。
const visibleMessages = computed(() => props.messages);

// 方向前缀是否显示（RX/TX 各自独立开关）
const showDirTag = (direction: string) =>
  direction === 'RX' ? props.filterRx : props.filterTx;

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

// 勾选自动滚动：只要有新消息就滚到最底下（无条件跟随）；同时刷新溢出状态
watch(
  () => visibleMessages.value[visibleMessages.value.length - 1]?.id,
  async () => {
    await nextTick();
    // rAF 里滚动，避免在同一帧内“改 DOM → 读布局”反复交错
    requestAnimationFrame(() => {
      const el = flowEl.value;
      if (!el) return;
      if (autoScroll.value) el.scrollTop = el.scrollHeight;
      updateScrollState();
    });
  }
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
      :class="{ anchored: visibleMessages.length > 0 }"
      :style="{ fontSize: fontSize + 'px' }"
      @scroll="onScroll"
      @wheel="onWheel"
    >
      <div v-if="visibleMessages.length === 0" class="flow-empty">等待接收数据</div>
      <!-- 两行式布局：第一行元信息（时间戳/方向），第二行起为数据正文；时间戳与 RX/TX 独立 -->
      <!-- v-memo：消息内容创建后不变，仅时间戳开关会影响渲染 → 未变化的行整行跳过 diff，
           高频接收时每帧 diff 成本从 O(全量 2000 行) 降为 O(1)，消除接收卡顿与点击迟钝 -->
      <div
        v-for="message in visibleMessages"
        :key="message.id"
        v-memo="[showTimestamp]"
        class="flow-line"
        :class="message.direction.toLowerCase()"
      >
        <div class="flow-meta">
          <span v-if="showTimestamp && message.time" class="flow-time">{{ message.time }}</span>
          <span v-if="showDirTag(message.direction)" class="flow-dir">{{ message.direction }}</span>
        </div>
        <div class="flow-text">{{ message.text }}</div>
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

.theme-dark .flow-empty {
  color: #6f737a;
}
</style>
