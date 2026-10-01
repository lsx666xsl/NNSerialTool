<script setup lang="ts">
import MessageFlow from './MessageFlow.vue';
import {
  autoScroll,
  clearSessionReceive,
  cycleNewline,
  exportSessionLog,
  filterHexInput,
  newlineLabel,
  sendData,
  sendSettings,
  splitSessions,
  toggleSendHex,
} from '../stores/appStore';

// 分屏视图：勾选的多个连接并排显示，各自独立的接收流、紧凑工具带与发送框。
// 工具带与单屏同序（自动滚动 / 自动换行 / 自动发送+间隔 / HEX / 导出 / 清空），
// 仅少"拓展命令"开关（拓展命令只在单屏生效）；时间戳与 RX/TX 前缀开关在连接列表卡片上设置。
// HEX 切换与输入过滤复用 appStore 共享实现（与单屏行为一致）。
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
        <!-- 紧凑工具带：顺序与单屏一致（自动滚动 / 自动换行 / 自动发送+间隔 / HEX / 导出 / 清空；无拓展命令） -->
        <div class="split-toolbar">
          <button
            class="mini-toggle"
            :class="{ on: autoScroll }"
            title="新消息到达时自动滚动到底部"
            @click="autoScroll = !autoScroll"
          >
            自动滚动
          </button>
          <button
            class="mini-toggle newline-cycle"
            :class="{ on: sendSettings.newline !== 'none' }"
            title="点击循环切换发送时附加的换行符"
            @click="cycleNewline()"
          >
            自动换行: {{ newlineLabel }}
          </button>
          <!-- 自动发送开关：开启后旁边显示可键入的间隔输入 -->
          <button
            class="mini-toggle"
            :class="{ on: sendSettings.loopSend }"
            title="按设定的间隔自动发送本面板发送框中的内容"
            @click="sendSettings.loopSend = !sendSettings.loopSend"
          >
            自动发送
          </button>
          <input
            v-if="sendSettings.loopSend"
            class="split-interval"
            :value="sendSettings.loopInterval"
            type="number"
            min="10"
            max="600000"
            title="自动发送间隔（毫秒），手动键入修改"
            @blur="(e) => (sendSettings.loopInterval = Math.min(600000, Math.max(10, Number((e.target as HTMLInputElement).value) || 1000)))"
            @keyup.enter="(e) => (e.target as HTMLInputElement).blur()"
          />
          <button
            class="mini-toggle"
            :class="{ on: session.sendHexMode ?? false }"
            title="发送框十六进制模式：输入按 HEX 解析以原始字节发送"
            @click="toggleSendHex(session)"
          >
            HEX
          </button>
          <button class="mini-toggle ghost-btn" title="导出当前消息框内容为日志文件" @click="exportSessionLog(session)">导出</button>
          <button class="mini-toggle ghost-btn" title="清空本面板消息" @click="clearSessionReceive(session)">清空</button>
        </div>
        <div class="split-send">
          <input
            v-model="session.sendText"
            :placeholder="session.sendHexMode ? '十六进制（如 41 42 43）' : '发送数据'"
            @input="filterHexInput(session, $event)"
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

/* 紧凑工具带：与单屏同序的精简版（自动滚动/自动换行/自动发送/HEX/导出/清空） */
.split-toolbar {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 4px 6px;
}

/* 自动发送间隔输入：随开启内联出现，手动键入（隐藏原生步进按钮），窄宽度不挤压按钮 */
.split-interval {
  width: 64px;
  padding: 3px 6px;
  font-size: 11px;
  text-align: center;
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

/* 导出/清空：与单屏一致的常亮蓝（不随开关变灰），同款迷你胶囊尺寸 */
.split-toolbar .ghost-btn {
  padding: 3px 8px;
  font-size: 11px;
  border-radius: 999px;
  background: rgba(59, 111, 212, 0.08);
  color: #3563c2;
  box-shadow: inset 0 0 0 1px rgba(59, 111, 212, 0.14);
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

.theme-dark .split-toolbar .ghost-btn {
  background: rgba(87, 157, 245, 0.12);
  color: #8fbdf7;
  box-shadow: inset 0 0 0 1px rgba(87, 157, 245, 0.18);
}
</style>
