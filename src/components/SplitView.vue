<script setup lang="ts">
import MessageFlow from './MessageFlow.vue';
import { bytesToHex, hexToBytes } from '../utils/format';
import {
  autoScroll,
  cycleNewline,
  exportSessionLog,
  newlineLabel,
  sendData,
  sendSettings,
  splitSessions,
} from '../stores/appStore';
import type { ConnectionSession } from '../types';

// 分屏视图：勾选的多个连接并排显示，各自独立的接收流、紧凑工具带与发送框。
// 工具带为单屏的精简版（分屏面板窄）：自动滚动 / 导出 / 自动换行 / HEX；
// 时间戳与 RX/TX 前缀开关在连接列表卡片上设置，此处不重复。

// 行内 HEX/文本切换：双向转换内容（字符串⇄HEX 字节流），空内容仅翻转模式
const toggleSendHex = (session: ConnectionSession) => {
  const toHex = !(session.sendHexMode ?? false);
  session.sendHexMode = toHex;
  if (!session.sendText.trim()) return;
  session.sendText = toHex
    ? bytesToHex(Array.from(new TextEncoder().encode(session.sendText)))
    : new TextDecoder().decode(new Uint8Array(hexToBytes(session.sendText)));
};
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
          :hex-mode="session.hexMode ?? false"
        />
        <!-- 紧凑工具带：自动滚动 / 导出 / 自动换行 / HEX（单屏工具带的精简版） -->
        <div class="split-toolbar">
          <button
            class="mini-toggle"
            :class="{ on: autoScroll }"
            title="新消息到达时自动滚动到底部"
            @click="autoScroll = !autoScroll"
          >
            自动滚动
          </button>
          <button class="mini-toggle" title="导出当前消息框内容为日志文件" @click="exportSessionLog(session)">导出</button>
          <button
            class="mini-toggle newline-cycle"
            :class="{ on: sendSettings.newline !== 'none' }"
            title="点击循环切换发送时附加的换行符"
            @click="cycleNewline()"
          >
            换行: {{ newlineLabel }}
          </button>
          <button
            class="mini-toggle"
            :class="{ on: session.sendHexMode ?? false }"
            title="发送框十六进制模式：输入按 HEX 解析以原始字节发送"
            @click="toggleSendHex(session)"
          >
            HEX
          </button>
        </div>
        <div class="split-send">
          <input
            v-model="session.sendText"
            :placeholder="session.sendHexMode ? '十六进制（如 41 42 43）' : '发送数据'"
            @keyup.enter="sendData(session)"
          />
          <button class="primary-btn" :disabled="session.status !== 'connected'" @click="sendData(session)">发送</button>
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
  /* 行高按可视区均分（下限 280px）：卡片高度固定，各自的消息区独立滚动 */
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

/* 紧凑工具带：单屏工具带的精简版（自动滚动/导出/自动换行/HEX） */
.split-toolbar {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 4px 6px;
}

.split-toolbar .mini-toggle {
  padding: 3px 8px;
  font-size: 11px;
  border-radius: 999px;
  background: rgba(23, 26, 33, 0.05);
  color: #8a9099;
  box-shadow: inset 0 0 0 1px rgba(23, 26, 33, 0.1);
}

.split-toolbar .mini-toggle.on {
  background: rgba(59, 111, 212, 0.12);
  color: #3563c2;
  box-shadow: inset 0 0 0 1px rgba(59, 111, 212, 0.3);
}

.split-toolbar .self-select {
  padding: 4px 8px;
  font-size: 11px;
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

.theme-dark .split-toolbar .mini-toggle {
  background: rgba(255, 255, 255, 0.06);
  color: #9da0a8;
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.1);
}

.theme-dark .split-toolbar .mini-toggle.on {
  background: rgba(87, 157, 245, 0.16);
  color: #8fbdf7;
  box-shadow: inset 0 0 0 1px rgba(87, 157, 245, 0.4);
}
</style>
