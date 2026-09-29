<script setup lang="ts">
import { computed, ref } from 'vue';
import MessageFlow from './MessageFlow.vue';
import NumberInput from './NumberInput.vue';
import SelfSelect from './SelfSelect.vue';
import {
  activeSession,
  addQuickCommand,
  autoScroll,
  clearSessionReceive,
  closeConnection,
  exportSessionLog,
  openConnection,
  quickCommands,
  removeQuickCommand,
  removeSession,
  sendData,
  sendQuickCommand,
  sendSettings,
} from '../stores/appStore';

// 单屏大视图：接收流 + 工具带 + 发送栏 + 右侧拓展命令纵栏（SSCOM 风格）。
// 旧"选项"折叠面板已移除：换行/发后清空并入工具带，快捷命令常驻右栏。
const connected = computed(() => activeSession.value?.status === 'connected');

// 拓展命令编辑模式：切换后按钮变成可编辑的名称/内容输入行
const cmdEditing = ref(false);

const newlineOptions = [
  { value: 'none', label: '无' },
  { value: 'lf', label: '\\n' },
  { value: 'crlf', label: '\\r\\n' },
  { value: 'cr', label: '\\r' },
];
</script>

<template>
  <section class="content-card detail-view">
    <div v-if="!activeSession" class="empty-box large">请选择或新建一个连接会话。</div>

    <template v-else>
      <!-- 头部：会话名 + 连接开关；已连接（红色关闭按钮）时右侧出现删除会话 X -->
      <div class="detail-header">
        <div class="detail-title">
          <h2>{{ activeSession.name }}</h2>
        </div>
        <div class="button-group">
          <button v-if="activeSession.status === 'closed'" class="primary-btn" @click="openConnection(activeSession)">打开连接</button>
          <button v-else class="danger-btn" @click="closeConnection(activeSession)">关闭连接</button>
          <button
            v-if="activeSession.status === 'connected'"
            class="x-session"
            title="删除当前会话"
            @click="removeSession(activeSession)"
          >
            <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round">
              <line x1="18" y1="6" x2="6" y2="18"></line>
              <line x1="6" y1="6" x2="18" y2="18"></line>
            </svg>
          </button>
        </div>
      </div>

      <!-- 收发合一面板：左（接收区+工具带+发送栏）+ 右（拓展命令纵栏） -->
      <div class="work-panel">
        <div class="work-main">
          <MessageFlow
            :messages="activeSession.messages"
            :show-timestamp="activeSession.showTimestamp ?? true"
            :filter-rx="activeSession.filterRx ?? true"
            :filter-tx="activeSession.filterTx ?? true"
          />

          <!-- 工具带：位于接收框与发送框之间 -->
          <div class="panel-toolbar">
            <button
              class="mini-toggle"
              :class="{ on: autoScroll }"
              title="新消息到达时自动滚动到底部（向上翻看时自动暂停）"
              @click="autoScroll = !autoScroll"
            >
              自动滚动
            </button>
            <button class="ghost-btn" title="把当前消息框全部内容（含时间戳/方向标签）导出为日志文件；保存路径可在设置中配置" @click="exportSessionLog(activeSession)">
              导出
            </button>
            <button class="ghost-btn" @click="clearSessionReceive(activeSession)">清空</button>
            <label class="toolbar-item" title="发送时附加的换行符">
              换行
              <SelfSelect v-model="sendSettings.newline" :full="false" :options="newlineOptions" />
            </label>
            <button
              class="mini-toggle toolbar-end"
              :class="{ on: sendSettings.clearAfterSend }"
              title="发送后自动清空输入框"
              @click="sendSettings.clearAfterSend = !sendSettings.clearAfterSend"
            >
              发后清空
            </button>
          </div>

          <!-- 发送栏：输入 + 自动发送 + 发送按钮（右下角框外）；窄窗口放不下时按钮换行 -->
          <div class="send-row">
            <textarea
              v-model="activeSession.sendText"
              class="send-input"
              rows="2"
              placeholder="输入要发送的数据（Ctrl+回车 发送）"
              @keydown.ctrl.enter.prevent="sendData(activeSession)"
            ></textarea>
            <label class="auto-send" title="按设定的间隔自动发送发送框中的内容（作用于开启时的会话）">
              <input type="checkbox" v-model="sendSettings.loopSend" />
              自动
              <NumberInput
                v-model="sendSettings.loopInterval"
                :min="10"
                :max="600000"
                :step="10"
                class="auto-interval"
                @click.stop
              />
              ms
            </label>
            <button class="primary-btn send-btn" :disabled="!connected" @click="sendData(activeSession)">发送</button>
          </div>
        </div>

        <!-- 拓展命令纵栏：点击即发送到当前会话；编辑模式下可改名/改内容/删除 -->
        <div class="cmd-strip">
          <div class="cmd-head">
            <span class="cmd-title">拓展命令</span>
            <button class="mini-toggle" :class="{ on: cmdEditing }" @click="cmdEditing = !cmdEditing">
              {{ cmdEditing ? '完成' : '编辑' }}
            </button>
          </div>
          <div class="cmd-list">
            <template v-if="!cmdEditing">
              <button
                v-for="(cmd, index) in quickCommands"
                :key="index"
                class="cmd-btn"
                :disabled="!connected"
                :title="cmd.text"
                @click="sendQuickCommand(cmd)"
              >
                {{ cmd.name || `命令${index + 1}` }}
              </button>
            </template>
            <template v-else>
              <div v-for="(cmd, index) in quickCommands" :key="index" class="cmd-edit">
                <input v-model="cmd.name" placeholder="名称" />
                <input v-model="cmd.text" placeholder="内容" />
                <button class="cmd-del" title="删除命令" @click="removeQuickCommand(index)">删除</button>
              </div>
            </template>
            <button class="cmd-add" title="添加命令" @click="addQuickCommand">＋</button>
          </div>
        </div>
      </div>
    </template>
  </section>
</template>

<style scoped>
.detail-view {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.detail-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.button-group {
  display: flex;
  align-items: center;
  gap: 12px;
}

/* 已连接时标题右侧的删除会话按钮：与红色关闭连接并排 */
.x-session {
  width: 30px;
  height: 30px;
  padding: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 6px;
  background: rgba(199, 69, 65, 0.1);
  color: #c74541;
  box-shadow: none;
}

.x-session:hover {
  background: rgba(199, 69, 65, 0.2);
  transform: none;
}

/* 收发合一面板：左主区 + 右拓展命令栏 */
.work-panel {
  flex: 1;
  min-height: 0;
  display: flex;
  gap: 10px;
  background: rgba(243, 244, 246, 0.7);
  border: 1px solid rgba(23, 26, 33, 0.07);
  border-radius: 8px;
  padding: 12px;
}

.work-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

/* 工具带：自动滚动/导出/清空/换行/发后清空 */
.panel-toolbar {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
  padding: 6px 2px;
  border-top: 1px dashed rgba(23, 26, 33, 0.12);
  flex-shrink: 0;
}

.mini-toggle {
  padding: 4px 10px;
  font-size: 12px;
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

.toolbar-item {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: #3b414b;
  white-space: nowrap;
}

.toolbar-end {
  margin-left: auto;
}

/* 发送栏：输入框 + 自动发送 + 发送按钮（右下角框外） */
.send-row {
  display: flex;
  align-items: flex-end;
  gap: 10px;
  flex-wrap: wrap;
  flex-shrink: 0;
}

.send-input {
  flex: 1;
  /* 允许在窄窗口下收缩，避免发送栏把面板撑出横向滚动 */
  min-width: 0;
  resize: none;
  min-height: 46px;
  max-height: 120px;
  font-family: Consolas, 'Courier New', monospace;
  line-height: 1.5;
}

.auto-send {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 13px;
  color: #3b414b;
  white-space: nowrap;
  padding-bottom: 8px;
}

.auto-interval {
  width: 92px;
  /* 窄窗口下允许收缩（最低 60px），给发送按钮让位，避免发送栏 4px 级横向溢出 */
  min-width: 60px;
  flex-shrink: 1;
}

.send-btn {
  padding: 10px 26px;
  flex-shrink: 0;
}

/* 拓展命令纵栏（SSCOM 风格）：命令按钮纵向排列，点击即发送 */
.cmd-strip {
  width: 150px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
  background: rgba(23, 26, 33, 0.03);
  border: 1px dashed rgba(23, 26, 33, 0.12);
  border-radius: 8px;
  padding: 8px;
}

.cmd-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 6px;
}

.cmd-title {
  font-size: 12px;
  color: #8a9099;
}

.cmd-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 6px;
  scrollbar-width: thin;
  scrollbar-color: rgba(128, 132, 140, 0.45) transparent;
}

.cmd-list::-webkit-scrollbar {
  width: 6px;
}

.cmd-list::-webkit-scrollbar-thumb {
  background: rgba(128, 132, 140, 0.45);
  border-radius: 3px;
}

.cmd-btn {
  padding: 7px 8px;
  font-size: 12px;
  background: rgba(59, 111, 212, 0.08);
  color: #3563c2;
  box-shadow: inset 0 0 0 1px rgba(59, 111, 212, 0.2);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.cmd-btn:hover:not(:disabled) {
  background: rgba(59, 111, 212, 0.16);
}

.cmd-edit {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding-bottom: 6px;
  border-bottom: 1px dashed rgba(23, 26, 33, 0.1);
}

.cmd-edit input {
  padding: 4px 6px;
  font-size: 12px;
}

.cmd-del {
  align-self: flex-start;
  padding: 2px 8px;
  font-size: 12px;
  color: #c74541;
  background: transparent;
  box-shadow: none;
}

.cmd-add {
  padding: 6px 0;
  font-size: 14px;
  background: rgba(23, 26, 33, 0.05);
  color: #6b7280;
  box-shadow: inset 0 0 0 1px rgba(23, 26, 33, 0.1);
}

/* 深色主题 */
.theme-dark .auto-send,
.theme-dark .toolbar-item {
  color: #9da0a8;
}

.theme-dark .work-panel {
  background: rgba(255, 255, 255, 0.04);
  border-color: rgba(255, 255, 255, 0.09);
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

.theme-dark .cmd-strip {
  background: rgba(255, 255, 255, 0.04);
  border-color: rgba(255, 255, 255, 0.1);
}

.theme-dark .cmd-title {
  color: #7c828c;
}

.theme-dark .cmd-btn {
  background: rgba(87, 157, 245, 0.12);
  color: #8fbdf7;
  box-shadow: inset 0 0 0 1px rgba(87, 157, 245, 0.25);
}

.theme-dark .cmd-btn:hover:not(:disabled) {
  background: rgba(87, 157, 245, 0.22);
}

.theme-dark .cmd-add {
  background: rgba(255, 255, 255, 0.06);
  color: #b3b7be;
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.1);
}
</style>
