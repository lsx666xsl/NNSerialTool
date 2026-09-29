<script setup lang="ts">
import {
  activeSessionId,
  connectedCount,
  removeSession,
  sessions,
  selectedSessionIds,
  toggleConnection,
  toggleSplitSession,
} from '../stores/appStore';
import { sessionPortLabel, sessionSubLabel } from '../utils/session';

// 连接列表：所有会话的卡片。
// 标题行 = 状态点（左）+ 名称 + 端口参数 + 删除 X；字节行 = TX/RX 统计；
// 开关行 = 分屏 / 总览 / Timestamp / RX / TX / 计数，全部为主题小按钮。
</script>

<template>
  <section class="panel session-list">
    <div class="section-title">
      <h2>连接列表</h2>
      <span>{{ connectedCount }}/{{ sessions.length }} 已连接</span>
    </div>

    <div v-if="sessions.length === 0" class="empty-box">还没有连接会话，请先添加一个。</div>

    <!-- 内部滚动：会话多时列表区域独立滚动，滚动条随内容溢出自动出现 -->
    <div class="session-scroll">
    <div
      v-for="session in sessions"
      :key="session.id"
      class="session-card"
      :class="{ active: session.id === activeSessionId }"
      @click="activeSessionId = session.id"
    >
      <!-- 标题行：状态点即连接开关（绿=已连接点击关闭，红=已关闭点击开启）；
           右侧为紧凑参数（波特率/端口）与删除 X -->
      <div class="session-main">
        <button
          class="status-dot"
          :class="session.status"
          :title="session.status === 'connected' ? '点击关闭连接' : '点击打开连接'"
          @click.stop="toggleConnection(session)"
        ></button>
        <strong class="session-name">{{ session.name }}</strong>
        <span class="name-extra">{{ sessionPortLabel(session) }}</span>
        <button class="x-btn" title="删除会话" @click.stop="removeSession(session)">
          <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round">
            <line x1="18" y1="6" x2="6" y2="18"></line>
            <line x1="6" y1="6" x2="18" y2="18"></line>
          </svg>
        </button>
      </div>
      <p class="session-sub">{{ sessionSubLabel(session) }}</p>

      <!-- 收发字节统计：按实际写出/读入的字节数累计（清空消息不清零） -->
      <div class="byte-line">
        <span class="tx">TX: {{ session.txBytes }} Byte</span>
        <span class="rx">RX: {{ session.rxBytes }} Byte</span>
      </div>

      <div class="session-actions">
        <!-- 会话级开关：分屏（勾选后进入分屏视图）/ 总览（收发汇入总线时间线，选择性加入）/
             Timestamp 时间戳 / RX、TX 方向过滤 -->
        <span class="toggles">
          <button
            class="mini-toggle"
            :class="{ on: selectedSessionIds.includes(session.id) }"
            title="加入分屏显示"
            @click.stop="toggleSplitSession(session.id)"
          >
            分屏
          </button>
          <button
            class="mini-toggle"
            :class="{ on: session.inBus ?? false }"
            title="加入总览：本会话收发汇入总线时间线"
            @click.stop="session.inBus = !(session.inBus ?? false)"
          >
            总览
          </button>
          <button
            class="mini-toggle"
            :class="{ on: session.showTimestamp ?? true }"
            title="Timestamp 时间戳显示开关"
            @click.stop="session.showTimestamp = !(session.showTimestamp ?? true)"
          >
            Timestamp
          </button>
          <button
            class="mini-toggle"
            :class="{ on: session.filterRx ?? true }"
            title="显示 RX 接收数据"
            @click.stop="session.filterRx = !(session.filterRx ?? true)"
          >
            RX
          </button>
          <button
            class="mini-toggle"
            :class="{ on: session.filterTx ?? true }"
            title="显示 TX 发送数据"
            @click.stop="session.filterTx = !(session.filterTx ?? true)"
          >
            TX
          </button>
        </span>
        <span class="msg-count">{{ session.messageCount }} 条</span>
      </div>
    </div>
    </div>
  </section>
</template>

<style scoped>
/* 内部滚动容器：面板占满侧栏剩余高度，会话卡片在内部滚动 */
.session-list {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.session-scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  /* 内容不溢出时不出现；溢出后细滚动条常驻可见 */
  scrollbar-width: thin;
  scrollbar-color: rgba(128, 132, 140, 0.45) transparent;
}

.session-scroll::-webkit-scrollbar {
  width: 8px;
}

.session-scroll::-webkit-scrollbar-thumb {
  background: rgba(128, 132, 140, 0.45);
  border-radius: 4px;
}

.session-scroll::-webkit-scrollbar-track {
  background: transparent;
}

.session-card {
  border: 1px solid rgba(23, 26, 33, 0.1);
  border-radius: 8px;
  padding: 12px;
  margin-top: 10px;
  cursor: pointer;
  background: rgba(255, 255, 255, 0.72);
  transition: transform 0.18s ease, border-color 0.18s ease, box-shadow 0.18s ease, background 0.18s ease;
}

.session-card:hover {
  transform: translateY(-1px);
  border-color: rgba(59, 111, 212, 0.35);
  box-shadow: 0 6px 18px rgba(15, 23, 42, 0.08);
}

.session-card.active {
  border-color: rgba(59, 111, 212, 0.55);
  background: rgba(59, 111, 212, 0.06);
  box-shadow: 0 6px 18px rgba(59, 111, 212, 0.12);
}

/* 标题行：状态点 + 名称 + 右上角删除 X */
.session-main {
  display: flex;
  align-items: center;
  gap: 8px;
}

.session-name {
  flex: 1;
  min-width: 0;
  font-size: 14px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.x-btn {
  flex-shrink: 0;
  width: 22px;
  height: 22px;
  padding: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 5px;
  background: transparent;
  color: #9ca3af;
  box-shadow: none;
}

.x-btn:hover {
  background: rgba(199, 69, 65, 0.1);
  color: #c74541;
}

.session-sub {
  margin: 4px 0 0 18px;
  font-size: 13px;
  color: #6b7280;
}

/* 标题行紧凑参数（波特率/端口）：位于 X 按钮左侧 */
.name-extra {
  flex-shrink: 0;
  font-size: 11px;
  color: #8a9099;
  font-variant-numeric: tabular-nums;
}

/* 收发字节统计行：与副标题同缩进，等宽数字避免跳动 */
.byte-line {
  display: flex;
  gap: 14px;
  margin: 7px 0 0 18px;
  font-size: 12px;
  font-variant-numeric: tabular-nums;
}

.byte-line .tx {
  color: #2b6cb0;
}

.byte-line .rx {
  color: #2e8b45;
}

/* 状态点即连接开关：hover 放大提示可点击 */
.status-dot {
  width: 12px;
  height: 12px;
  border-radius: 999px;
  flex-shrink: 0;
  padding: 0;
  cursor: pointer;
  transition: transform 0.15s ease, box-shadow 0.15s ease;
}

.status-dot:hover {
  transform: scale(1.3);
}

.status-dot.connected {
  background: #3fa35c;
  box-shadow: 0 0 6px rgba(63, 163, 92, 0.55);
}

.status-dot.closed {
  background: #d4534f;
  box-shadow: 0 0 6px rgba(212, 83, 79, 0.45);
}

.session-actions {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 4px 8px;
  margin-top: 8px;
  font-size: 13px;
  color: #6b7280;
}


.msg-count {
  margin-left: auto;
  white-space: nowrap;
}

/* 会话级显示开关：紧凑小按钮，激活态高亮 */
.toggles {
  display: inline-flex;
  gap: 4px;
}

.mini-toggle {
  padding: 2px 8px;
  font-size: 11px;
  border-radius: 999px;
  background: rgba(23, 26, 33, 0.05);
  color: #8a9099;
  box-shadow: inset 0 0 0 1px rgba(23, 26, 33, 0.1);
}

.mini-toggle.on {
  background: rgba(59, 111, 212, 0.12);
  color: #3563c2;
  box-shadow: inset 0 0 0 1px rgba(59, 111, 212, 0.3);
}

/* 深色主题 */
.theme-dark .session-card {
  background: rgba(255, 255, 255, 0.04);
  border-color: rgba(255, 255, 255, 0.09);
}

.theme-dark .session-card:hover {
  border-color: rgba(87, 157, 245, 0.4);
  box-shadow: 0 8px 22px rgba(0, 0, 0, 0.3);
}

.theme-dark .session-card.active {
  background: rgba(87, 157, 245, 0.1);
  border-color: rgba(87, 157, 245, 0.45);
  box-shadow: 0 8px 22px rgba(0, 0, 0, 0.3);
}

.theme-dark .session-sub,
.theme-dark .session-actions {
  color: #9da0a8;
}

.theme-dark .name-extra {
  color: #7c828c;
}

.theme-dark .byte-line .tx {
  color: #6ca7e8;
}

.theme-dark .byte-line .rx {
  color: #6bc97e;
}

.theme-dark .x-btn {
  color: #9da0a8;
}

.theme-dark .x-btn:hover {
  background: rgba(232, 128, 124, 0.14);
  color: #e8807c;
}


.theme-dark .mini-toggle {
  background: rgba(255, 255, 255, 0.06);
  color: #8a9099;
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.1);
}

.theme-dark .mini-toggle.on {
  background: rgba(87, 157, 245, 0.16);
  color: #8fbdf7;
  box-shadow: inset 0 0 0 1px rgba(87, 157, 245, 0.4);
}
</style>
