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
import { sessionSubLabel } from '../utils/session';

// 连接列表：所有会话的卡片。
// 标题行 = 状态点（左）+ 名称 + 删除 X（右上角）；开关行 = 连接开关 / 分屏 / 显示开关 / 计数。
</script>

<template>
  <section class="panel">
    <div class="section-title">
      <h2>连接列表</h2>
      <span>{{ connectedCount }}/{{ sessions.length }} 已连接</span>
    </div>

    <div v-if="sessions.length === 0" class="empty-box">还没有连接会话，请先添加一个。</div>

    <div
      v-for="session in sessions"
      :key="session.id"
      class="session-card"
      :class="{ active: session.id === activeSessionId }"
      @click="activeSessionId = session.id"
    >
      <!-- 标题行：状态点即连接开关（绿=已连接点击关闭，红=已关闭点击开启），删除 X 固定右上角 -->
      <div class="session-main">
        <button
          class="status-dot"
          :class="session.status"
          :title="session.status === 'connected' ? '点击关闭连接' : '点击打开连接'"
          @click.stop="toggleConnection(session)"
        ></button>
        <strong class="session-name">{{ session.name }}</strong>
        <button class="x-btn" title="删除会话" @click.stop="removeSession(session)">
          <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round">
            <line x1="18" y1="6" x2="6" y2="18"></line>
            <line x1="6" y1="6" x2="18" y2="18"></line>
          </svg>
        </button>
      </div>
      <p class="session-sub">{{ sessionSubLabel(session) }}</p>

      <div class="session-actions">
        <label class="check-label" @click.stop>
          <input
            type="checkbox"
            :checked="selectedSessionIds.includes(session.id)"
            @change="toggleSplitSession(session.id)"
          />
          分屏
        </label>
        <!-- 会话级显示开关：Timestamp 时间戳；RX/TX 独立方向过滤（只看收/只看发） -->
        <span class="toggles">
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
  </section>
</template>

<style scoped>
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

.check-label {
  font-size: 13px;
  color: #6b7280;
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
.theme-dark .session-actions,
.theme-dark .check-label {
  color: #9da0a8;
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
