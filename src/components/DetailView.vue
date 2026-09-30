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

// 单屏大视图：接收流 + 工具带 + 发送区（高度可拖拽）+ 右侧拓展命令纵栏。
// 旧"选项"折叠面板已移除：自动换行/自动发送并入工具带，快捷命令常驻右栏。
const connected = computed(() => activeSession.value?.status === 'connected');

// 拓展命令编辑模式：切换后按钮变成可编辑的名称/内容输入行
const cmdEditing = ref(false);
// 右侧拓展命令栏显示开关（默认开启）
const cmdStripVisible = ref(true);

// 发送框高度（可拖动上边界调整；拖高发送框时接收框自动收缩，二者互斥共享空间）
const sendHeight = ref(64);

const onResizeHandleDown = (e: PointerEvent) => {
  e.preventDefault();
  const startY = e.clientY;
  const startHeight = sendHeight.value;
  const onMove = (ev: PointerEvent) => {
    // 向上拖 = 增高；限制在 46~340px
    sendHeight.value = Math.min(340, Math.max(46, startHeight - (ev.clientY - startY)));
  };
  const onUp = () => {
    window.removeEventListener('pointermove', onMove);
    window.removeEventListener('pointerup', onUp);
  };
  window.addEventListener('pointermove', onMove);
  window.addEventListener('pointerup', onUp);
};

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
      <!-- 头部：会话名 + 连接开关；未连接（蓝色打开按钮）时右侧出现删除会话 X -->
      <div class="detail-header">
        <div class="detail-title">
          <h2>{{ activeSession.name }}</h2>
        </div>
        <div class="button-group">
          <button v-if="activeSession.status === 'closed'" class="primary-btn" @click="openConnection(activeSession)">打开连接</button>
          <button v-else class="danger-btn" @click="closeConnection(activeSession)">关闭连接</button>
          <button
            v-if="activeSession.status === 'closed'"
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
              class="tb-toggle"
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
              自动换行
              <SelfSelect v-model="sendSettings.newline" :full="false" :options="newlineOptions" />
            </label>
            <!-- 自动发送组：与自动换行同栏 -->
            <label class="auto-send-group" title="按设定的间隔自动发送发送框中的内容（作用于开启时的会话）">
              <input type="checkbox" v-model="sendSettings.loopSend" />
              自动发送
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
          </div>

          <!-- 发送区：上边界可上下拖动拉伸（与接收框互斥共享空间） -->
          <div class="send-area">
            <div class="send-resize" title="拖动调整发送框高度" @pointerdown="onResizeHandleDown"><span></span></div>
            <!-- 发送栏：输入 + 拓展命令开关 + 发送按钮（拓展命令栏在面板右侧） -->
            <div class="send-row">
              <textarea
                v-model="activeSession.sendText"
                class="send-input"
                rows="2"
                :style="{ height: sendHeight + 'px' }"
                placeholder="输入要发送的数据（Ctrl+回车 发送）"
                @keydown.ctrl.enter.prevent="sendData(activeSession)"
              ></textarea>
              <button
                class="tb-toggle send-side-toggle"
                :class="{ on: cmdStripVisible }"
                title="显示/隐藏右侧拓展命令栏"
                @click="cmdStripVisible = !cmdStripVisible"
              >
                拓展命令
              </button>
              <button class="primary-btn send-btn" :disabled="!connected" @click="sendData(activeSession)">发送</button>
            </div>
          </div>
        </div>

        <!-- 拓展命令纵栏：点击即发送到当前会话；编辑模式下可改名/改内容/删除 -->
        <div v-if="cmdStripVisible" class="cmd-strip">
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

/* 已连接/未连接时标题右侧的删除会话按钮：与连接按钮同款红色样式 */
.x-session {
  width: 40px;
  height: 40px;
  padding: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 6px;
  background: linear-gradient(180deg, #c74541, #b93b37);
  color: #ffffff;
  box-shadow: 0 4px 12px rgba(199, 69, 65, 0.3);
}

.x-session:hover:not(:disabled) {
  background: linear-gradient(180deg, #b93b37, #a83330);
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

/* 工具带：自动滚动/导出/清空/自动换行/自动发送组/拓展命令开关 */
.panel-toolbar {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
  padding: 6px 2px;
  border-top: 1px dashed rgba(23, 26, 33, 0.12);
  flex-shrink: 0;
}

/* 工具带矩形开关：与导出/清空等高（同内边距/字号/圆角），激活态蓝色高亮 */
.tb-toggle {
  padding: 8px 14px;
  font-size: 13px;
  border-radius: 6px;
  background: rgba(23, 26, 33, 0.05);
  color: #6b7280;
  box-shadow: inset 0 0 0 1px rgba(23, 26, 33, 0.1);
}

.tb-toggle.on {
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

/* 发送栏：输入框 + 发送按钮 + 拓展命令栏（与发送框等高） */
.send-row {
  display: flex;
  align-items: flex-end;
  gap: 10px;
  flex-shrink: 0;
}

/* 发送区：上边界拖拽手柄 + 发送行；拖高时接收框自动收缩（flex 互斥） */
.send-area {
  flex-shrink: 0;
}

.send-resize {
  height: 14px;
  /* 命中区比视觉条更高更宽，方便鼠标抓取 */
  margin: 0 -6px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: ns-resize;
  border-radius: 3px;
}

.send-resize span {
  width: 40px;
  height: 3px;
  border-radius: 2px;
  background: rgba(23, 26, 33, 0.15);
  transition: background 0.15s ease;
}

.send-resize:hover span {
  background: rgba(59, 111, 212, 0.55);
}

.send-input {
  flex: 1;
  /* 允许在窄窗口下收缩，避免发送栏把面板撑出横向滚动；高度由拖拽手柄控制 */
  min-width: 0;
  resize: none;
  line-height: 1.5;
}

/* 自动发送组：与自动换行同栏，无边框（同属发送参数区） */
.auto-send-group {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: #3b414b;
  white-space: nowrap;
  cursor: pointer;
}

.auto-interval {
  width: 92px;
  /* 窄窗口下允许收缩（最低 60px），给发送按钮让位，避免发送栏 4px 级横向溢出 */
  min-width: 60px;
  flex-shrink: 1;
}

/* 发送行内的拓展命令开关：与发送按钮等高、紧邻排列 */
.send-side-toggle {
  padding: 10px 14px;
  font-size: 13px;
  border-radius: 6px;
  flex-shrink: 0;
}

.send-side-toggle.on {
  background: rgba(59, 111, 212, 0.12);
  color: #3563c2;
  box-shadow: inset 0 0 0 1px rgba(59, 111, 212, 0.3);
}

.send-side-toggle:not(.on) {
  background: rgba(23, 26, 33, 0.05);
  color: #6b7280;
  box-shadow: inset 0 0 0 1px rgba(23, 26, 33, 0.1);
}

.send-btn {
  padding: 10px 26px;
  flex-shrink: 0;
}

/* 拓展命令纵栏（SSCOM 风格）：命令按钮纵向排列，点击即发送 */
.cmd-strip {
  width: 150px;
  /* 与左侧发送消息框保持等高（随拖拽手柄同步变化） */
  align-self: stretch;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
  background: rgba(23, 26, 33, 0.03);
  border: 1px dashed rgba(23, 26, 33, 0.12);
  border-radius: 8px;
  padding: 10px;
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
.theme-dark .auto-send-group,
.theme-dark .toolbar-item {
  color: #9da0a8;
}


.theme-dark .send-resize span {
  background: rgba(255, 255, 255, 0.18);
}

.theme-dark .send-resize:hover span {
  background: rgba(87, 157, 245, 0.6);
}

.theme-dark .work-panel {
  background: rgba(255, 255, 255, 0.04);
  border-color: rgba(255, 255, 255, 0.09);
}

.theme-dark .tb-toggle {
  background: rgba(255, 255, 255, 0.06);
  color: #b3b7be;
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.1);
}

.theme-dark .tb-toggle.on {
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
