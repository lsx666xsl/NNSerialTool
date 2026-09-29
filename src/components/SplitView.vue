<script setup lang="ts">
import MessageFlow from './MessageFlow.vue';
import { sendData, splitSessions } from '../stores/appStore';

// 分屏视图：勾选的多个连接并排显示，各自独立的接收流与发送框。
</script>

<template>
  <section class="content-card split-view">
    <div v-if="splitSessions.length === 0" class="empty-box large">请在左侧连接列表勾选要分屏显示的会话。</div>

    <div v-else class="split-grid" :class="`count-${Math.min(splitSessions.length, 4)}`">
      <article v-for="session in splitSessions" :key="session.id" class="split-card">
        <div class="panel-title">
          <div>
            <h3>{{ session.name }}</h3>
            <p>{{ session.status === 'connected' ? '已连接' : '未连接' }}</p>
          </div>
          <span class="status-dot" :class="session.status"></span>
        </div>
        <MessageFlow
          :messages="session.messages"
          :show-timestamp="session.showTimestamp ?? true"
          :filter-rx="session.filterRx ?? true"
          :filter-tx="session.filterTx ?? true"
        />
        <div class="split-send">
          <input v-model="session.sendText" placeholder="发送数据" @keyup.enter="sendData(session)" />
          <button class="primary-btn" @click="sendData(session)">发送</button>
        </div>
      </article>
    </div>
  </section>
</template>

<style scoped>
.split-view {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.split-grid {
  flex: 1;
  min-height: 0;
  display: grid;
  gap: 12px;
  overflow: auto;
  /* 分屏网格滚动条隐藏，卡片内容各自滚动 */
  scrollbar-width: none;
}

.split-grid::-webkit-scrollbar {
  display: none;
}

.split-grid.count-1 {
  grid-template-columns: 1fr;
}

.split-grid.count-2,
.split-grid.count-3,
.split-grid.count-4 {
  grid-template-columns: repeat(2, minmax(280px, 1fr));
  /* 行高按可视区均分（下限 280px）：卡片高度固定，各自的消息区独立滚动，
     会话多到超出可视区时整个网格才滚动（修复"窗口太多往下滚不动"——
     旧实现卡片高度随内容无限生长，滚轮永远被卡片内部的消息区吃掉） */
  grid-auto-rows: minmax(280px, 1fr);
}

/* 窄窗口（≤1100px，含默认 800x600）：两列各剩 300px 出头没法看，
   改为纵向堆叠，卡片占满整行，网格自身纵向滚动 */
@media (max-width: 1100px) {
  .split-grid.count-2,
  .split-grid.count-3,
  .split-grid.count-4 {
    grid-template-columns: minmax(0, 1fr);
    grid-auto-rows: minmax(280px, 1fr);
  }
}

.split-card {
  min-height: 240px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  background: rgba(243, 244, 246, 0.7);
  border: 1px solid rgba(23, 26, 33, 0.07);
  border-radius: 8px;
  padding: 12px;
}

.panel-title {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.panel-title h3 {
  margin: 0;
  font-size: 14px;
}

.panel-title p {
  margin: 3px 0 0;
  font-size: 13px;
  color: #6b7280;
}

.status-dot {
  width: 10px;
  height: 10px;
  border-radius: 999px;
  background: #9ca3af;
  flex-shrink: 0;
}

.status-dot.connected {
  background: #3fa35c;
}

.split-send {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}

/* 窄卡片里输入框可收缩，发送按钮不换行，避免横向溢出 */
.split-send input {
  flex: 1;
  min-width: 0;
}

/* 深色主题 */
.theme-dark .split-card {
  background: rgba(255, 255, 255, 0.04);
  border-color: rgba(255, 255, 255, 0.09);
}

.theme-dark .panel-title p {
  color: #9da0a8;
}
</style>
