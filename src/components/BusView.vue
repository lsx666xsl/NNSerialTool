<script setup lang="ts">
import SelfSelect from './SelfSelect.vue';
import {
  busDirectionFilter,
  busKeyword,
  busSessionFilter,
  filteredGlobalMessages,
  globalMessages,
  sessions,
} from '../stores/appStore';

// 总线视图：所有连接的收发记录汇成一条时间线，支持按连接/方向/关键字过滤。
</script>

<template>
  <section class="content-card global-view">
    <div class="filter-bar">
      <label>
        连接
        <SelfSelect
          v-model="busSessionFilter"
          :full="false"
          :options="sessions.map((s) => ({ value: s.id, label: s.name }))"
        />
      </label>
      <label>
        方向
        <SelfSelect
          v-model="busDirectionFilter"
          :full="false"
          :options="[
            { value: 'all', label: '全部' },
            { value: 'RX', label: 'RX' },
            { value: 'TX', label: 'TX' },
            { value: 'INFO', label: 'INFO' },
          ]"
        />
      </label>
      <label>
        关键字
        <input v-model="busKeyword" placeholder="搜索内容或连接名" />
      </label>
      <button class="ghost-btn" @click="globalMessages = []">清空总线</button>
    </div>

    <div class="message-list">
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

.filter-bar {
  display: flex;
  align-items: center;
  justify-content: flex-start;
  gap: 12px;
  flex-wrap: wrap;
}

.filter-bar label {
  min-width: 150px;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: #3b414b;
}

.filter-bar .self-select {
  min-width: 130px;
}

.message-list {
  flex: 1;
  min-height: 0;
  overflow: auto;
  border: 1px solid rgba(23, 26, 33, 0.1);
  border-radius: 6px;
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
  grid-template-columns: 104px 120px 46px 1fr;
  gap: 10px;
  padding: 5px 8px;
  border-radius: 4px;
  color: #23262b;
  white-space: pre-wrap;
  word-break: break-all;
}

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

.message-line.info .direction {
  color: #b7791f;
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

.theme-dark .message-line.info .direction {
  color: #d9a55a;
}
</style>
