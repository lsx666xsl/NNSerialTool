<script setup lang="ts">
import { nextTick, onUnmounted, ref, watch } from 'vue';
import SelfSelect from './SelfSelect.vue';
import {
  busDirectionFilter,
  busKeyword,
  busSessionFilter,
  filteredGlobalMessages,
  globalMessages,
  sessions,
} from '../stores/appStore';
import { readStorage, writeStorage } from '../utils/storage';

// 总线视图：已加入总览的会话收发记录汇成一条时间线。
// 过滤栏：连接（含常驻"全部"）/ 方向 / 关键字 + 自动滚动开关；
// 列表上方有列头（时间 / 来源 / 方向 / 内容），列表随新消息自动滚动（可关）。

// 总线自动滚动：默认开启（日志流视图），可点击关闭；持久化到 localStorage
const busAutoScroll = ref(readStorage<boolean>('st-bus-autoscroll', true));
watch(busAutoScroll, (v) => writeStorage('st-bus-autoscroll', v));

const listEl = ref<HTMLElement | null>(null);

// 新消息到达（或过滤结果变化）时滚动到底部
watch(
  () => filteredGlobalMessages.value[filteredGlobalMessages.value.length - 1]?.id,
  async () => {
    if (!busAutoScroll.value) return;
    await nextTick();
    const el = listEl.value;
    if (el) el.scrollTop = el.scrollHeight;
  }
);

onUnmounted(() => {
  writeStorage('st-bus-autoscroll', busAutoScroll.value);
});
</script>

<template>
  <section class="content-card global-view">
    <div class="filter-bar">
      <div class="filter-col">
        <span class="filter-label">连接</span>
        <SelfSelect
          v-model="busSessionFilter"
          :options="[{ value: 'all', label: '全部' }, ...sessions.map((s) => ({ value: s.id, label: s.name }))]"
          placeholder="全部"
        />
      </div>
      <div class="filter-col">
        <span class="filter-label">方向</span>
        <SelfSelect
          v-model="busDirectionFilter"
          :options="[
            { value: 'all', label: '全部' },
            { value: 'RX', label: 'RX' },
            { value: 'TX', label: 'TX' },
          ]"
        />
      </div>
      <div class="filter-col">
        <span class="filter-label">关键字</span>
        <input v-model="busKeyword" placeholder="搜索内容或连接名" />
      </div>
      <button
        class="ghost-btn filter-clear bus-autoscroll"
        :class="{ on: busAutoScroll }"
        :title="busAutoScroll ? '自动滚动已开启（新消息自动滚到底部）' : '自动滚动已关闭，点击开启'"
        @click="busAutoScroll = !busAutoScroll"
      >
        自动滚动
      </button>
      <button class="ghost-btn filter-clear" title="清空总线全部消息记录" @click="globalMessages = []">清空总线</button>
    </div>

    <div class="message-list" ref="listEl">
      <!-- 列头置于滚动容器内部（sticky 吸顶）：与数据行共享同一内边距，任何状态下像素级对齐 -->
      <div class="list-header">
        <span class="h-time">时间</span>
        <span class="h-source">来源</span>
        <span class="h-dir">方向</span>
        <span class="h-payload">内容</span>
      </div>
      <div v-if="filteredGlobalMessages.length === 0" class="empty-box">暂无匹配消息。</div>
      <div v-for="message in filteredGlobalMessages" :key="message.id" class="message-line" :class="message.direction.toLowerCase()">
        <span class="time">{{ message.time }}</span>
        <span class="source">{{ message.sessionName }}</span>
        <span class="direction">{{ message.direction }}</span>
        <span class="payload">{{ message.text }}</span>
      </div>
    </div>
  </section>
</template>

<style scoped>
.global-view {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

/* 过滤栏：左中右三列均分（自动滚动按钮自适应宽），标签在控件上方 */
.filter-bar {
  display: grid;
  /* minmax(0,1fr)：允许三列收缩到内容以下，长连接名靠省略号截断，窄窗口不溢出 */
  grid-template-columns: repeat(3, minmax(0, 1fr)) auto auto;
  gap: 10px;
  align-items: end;
}

.filter-col {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.filter-label {
  font-size: 12px;
  color: #6b7280;
}

.filter-col :deep(.self-select),
.filter-col input {
  width: 100%;
}

.filter-clear {
  white-space: nowrap;
  align-self: end;
}

/* 总线自动滚动开关：开启态高亮（与工具带按钮主题一致） */
.bus-autoscroll.on {
  background: rgba(59, 111, 212, 0.12);
  color: #3563c2;
  box-shadow: inset 0 0 0 1px rgba(59, 111, 212, 0.3);
}

/* 列头：与消息行同栅格对齐，指示各列内容类型 */
.list-header {
  position: sticky;
  top: 0;
  z-index: 1;
  display: grid;
  grid-template-columns: minmax(0, 104px) minmax(0, 120px) 46px minmax(60px, 1fr);
  gap: 10px;
  padding: 6px 8px;
  font-size: 12px;
  color: #6b7280;
  border-bottom: 1px solid rgba(23, 26, 33, 0.12);
  background: #f8f9fb;
  user-select: none;
}

.message-list {
  flex: 1;
  min-height: 0;
  overflow: auto;
  border: 1px solid rgba(23, 26, 33, 0.1);
  border-radius: 0 0 6px 6px;
  /* 跟随主题：浅色浅底深字 */
  background: #f8f9fb;
  color: #23262b;
  padding: 10px;
  font-family: Consolas, 'Courier New', monospace;
  font-size: 12px;
  /* 总线同样是数据信息区，滚动条保留并美化 */
  scrollbar-width: thin;
  scrollbar-color: rgba(128, 132, 140, 0.45) transparent;
}

.message-list::-webkit-scrollbar {
  width: 8px;
}

.message-list::-webkit-scrollbar-thumb {
  background: rgba(128, 132, 140, 0.45);
  border-radius: 4px;
}

.message-list::-webkit-scrollbar-track {
  background: transparent;
}

.message-line {
  display: grid;
  /* minmax(0, Npx)：时间/来源列窄窗口时可收缩并让位；正文列保底 60px 不被挤没 */
  grid-template-columns: minmax(0, 104px) minmax(0, 120px) 46px minmax(60px, 1fr);
  gap: 10px;
  padding: 5px 8px;
  border-radius: 4px;
  color: #23262b;
  white-space: pre-wrap;
  word-break: break-all;
}

/* 来源列：长会话名截断显示，不折行 */
.message-line .source {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.message-line:hover {
  background: rgba(23, 26, 33, 0.04);
}

.message-line.rx .direction {
  color: #2e8b45;
}

.message-line.tx .direction {
  color: #2b6cb0;
}

.time,
.source {
  color: #7c828c;
}

/* 深色主题 */
.theme-dark .filter-bar label {
  color: #9da0a8;
}

.theme-dark .message-list {
  background: #1a1b1e;
  border-color: rgba(255, 255, 255, 0.09);
  color: #d6d9de;
}

.theme-dark .message-line {
  color: #d6d9de;
}

.theme-dark .message-line:hover {
  background: rgba(255, 255, 255, 0.05);
}

.theme-dark .message-line.rx .direction {
  color: #6bc97e;
}

.theme-dark .message-line.tx .direction {
  color: #6ca7e8;
}

.theme-dark .list-header {
  background: #222427;
  border-color: rgba(255, 255, 255, 0.09);
  color: #9da0a8;
}
</style>
