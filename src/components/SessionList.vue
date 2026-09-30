<script setup lang="ts">
import { computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import {
  activeSessionId,
  closeConnection,
  connectedCount,
  notify,
  openConnection,
  removeSession,
  sessions,
  selectedSessionIds,
  toggleConnection,
  toggleSplitSession,
} from '../stores/appStore';
import { sessionSubLabel } from '../utils/session';

// 连接列表：所有会话的卡片，四行布局：
// ① 状态点（即连接开关）+ 名称 + 删除 X（始终显示）
// ② 连接路径（ip:port->ip:port / 波特率矩形框可实时编辑）
// ③ 收发字节统计兼显示开关（点击 Tx/Rx 矩形按钮切换方向数据是否显示）
// ④ 开关行：分屏 / 总览 / 时间戳 + 消息计数
// 标题行右侧 = 一键打开/关闭列表中的全部连接。

// 全部已连接时按钮变为"全部关闭"，否则为"全部打开"
const allConnected = computed(() => sessions.value.length > 0 && connectedCount.value === sessions.value.length);

const toggleAllSessions = () => {
  for (const session of sessions.value) {
    if (allConnected.value) {
      if (session.status === 'connected') closeConnection(session);
    } else if (session.status === 'closed') {
      openConnection(session);
    }
  }
};

// 行2 实时编辑：修改串口波特率（连接中经 serial_set_baudrate 实时生效，无需重连）
const onBaudChange = async (session: import('../types').ConnectionSession, e: Event) => {
  const val = Number((e.target as HTMLInputElement).value);
  if (!val || val <= 0 || val === session.config.baudRate) return;
  session.config.baudRate = val;
  if (session.status !== 'connected') {
    notify(`波特率已更新为 ${val}bps（打开连接后生效）`);
    return;
  }
  try {
    const msg = await invoke<string>('serial_set_baudrate', { port: session.config.port, baudRate: val });
    notify(msg);
  } catch (err) {
    notify(`修改波特率失败: ${err}`);
  }
};


</script>

<template>
  <section class="panel session-list">
    <div class="section-title">
      <h2>连接列表</h2>
      <span class="list-side">
        <button
          class="list-toggle"
          :disabled="sessions.length === 0"
          :title="allConnected ? '关闭列表中的全部连接' : '打开列表中的全部连接'"
          @click.stop="toggleAllSessions"
        >
          {{ allConnected ? '全部关闭' : '全部打开' }}
        </button>
        <span>{{ connectedCount }}/{{ sessions.length }} 已连接</span>
      </span>
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
        <!-- 行1：状态点即连接开关（绿=已连接点击关闭，红=已关闭点击开启）；X 最靠右 -->
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

        <!-- 行2：连接路径（可实时编辑）：串口为波特率矩形框；网络为远程端口矩形框 -->
        <p class="session-sub">
          <template v-if="session.type === 'serial'">
            <span class="path-label">Baudrate:</span>
            <input
              class="path-edit"
              :value="session.config.baudRate"
              type="number"
              min="1"
              title="修改波特率（连接中实时生效）"
              @click.stop
              @change="onBaudChange(session, $event)"
            />
            <span class="path-unit">bps</span>
          </template>
          <template v-else>{{ sessionSubLabel(session) }}</template>
        </p>

        <!-- 行3：收发字节统计，按钮即开关——点击切换 TX/RX 数据是否显示（清空消息不清零计数） -->
        <div class="byte-line">
          <button
            class="byte-toggle tx"
            :class="{ off: !(session.filterTx ?? true) }"
            title="TX 发送数据显示开关"
            @click.stop="session.filterTx = !(session.filterTx ?? true)"
          >
            Tx: {{ session.txBytes }} B
          </button>
          <button
            class="byte-toggle rx"
            :class="{ off: !(session.filterRx ?? true) }"
            title="RX 接收数据显示开关"
            @click.stop="session.filterRx = !(session.filterRx ?? true)"
          >
            Rx: {{ session.rxBytes }} B
          </button>
        </div>

        <!-- 行4：分屏 / 总览 / 时间戳开关 + 消息计数 -->
        <div class="session-actions">
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
              title="时间戳显示开关"
              @click.stop="session.showTimestamp = !(session.showTimestamp ?? true)"
            >
              时间戳
            </button>
          </span>
          <span class="msg-count">{{ session.messageCount }} 条</span>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
/* 标题行右侧：一键开关 + 计数 */
.list-side {
  display: flex;
  align-items: center;
  gap: 8px;
}

.list-toggle {
  padding: 3px 10px;
  font-size: 12px;
  border-radius: 5px;
  background: rgba(59, 111, 212, 0.1);
  color: #3563c2;
  box-shadow: inset 0 0 0 1px rgba(59, 111, 212, 0.25);
}

.list-toggle:hover:not(:disabled) {
  background: rgba(59, 111, 212, 0.2);
}

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
  padding: 12px 14px;
  margin-top: 12px;
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

/* 行1：状态点 + 名称 + 删除 X（最靠右） */
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

/* 行2：连接路径 ip:port->ip:port */
.session-sub {
  margin: 6px 0 0;
  font-size: 12px;
  line-height: 1.7;
  color: #3b414b;
  font-variant-numeric: tabular-nums;
  display: flex;
  align-items: center;
  gap: 4px;
  overflow: hidden;
  white-space: nowrap;
}

/* 路径标签文字：与会话名同色系，不再浅灰 */
.path-label {
  color: #3b414b;
  flex-shrink: 0;
}

/* 行内可编辑矩形框：波特率 / 远程端口 */
.path-edit {
  width: 64px;
  padding: 1px 5px;
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  border-radius: 4px;
  background: rgba(23, 26, 33, 0.04);
  color: #23262b;
  box-shadow: inset 0 0 0 1px rgba(23, 26, 33, 0.14);
}

.path-edit:hover {
  box-shadow: inset 0 0 0 1px rgba(59, 111, 212, 0.4);
}

.path-edit:focus {
  outline: none;
  background: #ffffff;
  box-shadow: inset 0 0 0 1px rgba(59, 111, 212, 0.6);
}

.path-unit {
  flex-shrink: 0;
  color: #3b414b;
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

/* 行3：收发字节统计兼显示开关（矩形按钮；关闭态变灰） */
.byte-line {
  display: flex;
  gap: 8px;
  margin: 8px 0 0;
}

.byte-toggle {
  padding: 2px 8px;
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  border-radius: 4px;
}

.byte-toggle.tx {
  background: rgba(43, 108, 176, 0.08);
  color: #2b6cb0;
  box-shadow: inset 0 0 0 1px rgba(43, 108, 176, 0.3);
}

.byte-toggle.rx {
  background: rgba(46, 139, 69, 0.08);
  color: #2e8b45;
  box-shadow: inset 0 0 0 1px rgba(46, 139, 69, 0.3);
}

.byte-toggle.off {
  background: rgba(23, 26, 33, 0.04);
  color: #b6bbc3;
  box-shadow: inset 0 0 0 1px rgba(23, 26, 33, 0.1);
  
}

/* 行4：开关行 */
.session-actions {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 4px 6px;
  margin-top: 10px;
  font-size: 13px;
  color: #6b7280;
}

.toggles {
  display: inline-flex;
  flex-wrap: wrap;
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

.msg-count {
  margin-left: auto;
  white-space: nowrap;
  font-size: 12px;
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
  color: #c6c9cf;
}

.theme-dark .path-label,
.theme-dark .path-unit {
  color: #c6c9cf;
}

.theme-dark .path-edit {
  background: rgba(255, 255, 255, 0.06);
  color: #dfdfe3;
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.12);
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

.theme-dark .byte-toggle.tx {
  background: rgba(108, 167, 232, 0.12);
  color: #6ca7e8;
  box-shadow: inset 0 0 0 1px rgba(108, 167, 232, 0.35);
}

.theme-dark .byte-toggle.rx {
  background: rgba(107, 201, 126, 0.12);
  color: #6bc97e;
  box-shadow: inset 0 0 0 1px rgba(107, 201, 126, 0.35);
}

.theme-dark .byte-toggle.off {
  background: rgba(255, 255, 255, 0.04);
  color: #6f737a;
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.1);
}

.theme-dark .list-toggle {
  background: rgba(87, 157, 245, 0.16);
  color: #8fbdf7;
  box-shadow: inset 0 0 0 1px rgba(87, 157, 245, 0.4);
}
</style>
