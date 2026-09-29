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
  sendQuickCommand,
  sendData,
  sendSettings,
  sessions,
} from '../stores/appStore';
import { sessionTypeLabel } from '../utils/session';

// 单屏视图（参考 VOFA+ 布局）：连接参数在左侧卡片中，这里只保留收发主战场——
// 一个矩形面板：上方为流式接收区，下方为发送栏（输入 + 发送 + 可折叠发送选项）。
const showSendOptions = ref(false);

// 转发目标：桥接可选的 forwardTo 字段（undefined=不转发）与 SelfSelect 的字符串值
const forwardTarget = computed({
  get: () => activeSession.value?.forwardTo ?? '',
  set: (v) => {
    if (activeSession.value) activeSession.value.forwardTo = String(v);
  },
});

// 发送按钮与快捷命令仅在已连接时可用
const connected = computed(() => activeSession.value?.status === 'connected');
</script>

<template>
  <section class="content-card detail-view">
    <div v-if="!activeSession" class="empty-box large">请选择或新建一个连接会话。</div>

    <template v-else>
      <!-- 头部：会话名与状态信息 + 连接开关 -->
      <div class="detail-header">
        <div class="detail-title">
          <h2>{{ activeSession.name }}</h2>
          <p>{{ sessionTypeLabel(activeSession.type) }} · {{ activeSession.statusMsg }}</p>
        </div>
        <div class="button-group">
          <button v-if="activeSession.status === 'closed'" class="primary-btn" @click="openConnection(activeSession)">打开连接</button>
          <button v-else class="danger-btn" @click="closeConnection(activeSession)">关闭连接</button>
        </div>
      </div>

      <!-- 收发合一面板：接收区 + 工具带（收发之间）+ 发送栏 -->
      <div class="work-panel">
        <MessageFlow
          :messages="activeSession.messages"
          :show-timestamp="activeSession.showTimestamp ?? true"
          :filter-rx="activeSession.filterRx ?? true"
          :filter-tx="activeSession.filterTx ?? true"
        />

        <!-- 工具带：位于接收框与发送框之间 -->
        <div class="panel-toolbar">
          <label class="check-label" title="新消息到达时自动滚动到底部（向上翻看时自动暂停）">
            <input type="checkbox" v-model="autoScroll" />
            自动滚动
          </label>
          <button class="ghost-btn" title="把当前消息框全部内容（含时间戳/方向标签）导出为日志文件；保存路径可在设置中配置" @click="exportSessionLog(activeSession)">
            导出
          </button>
          <button class="ghost-btn" @click="clearSessionReceive(activeSession)">清空</button>
          <button class="ghost-btn toolbar-option" title="换行符 / 快捷命令 / 消息转发" @click="showSendOptions = !showSendOptions">
            选项 {{ showSendOptions ? '▴' : '▾' }}
          </button>
        </div>

        <!-- 发送栏：输入 + 自动发送 + 发送按钮（右下角框外） -->
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

        <div v-if="showSendOptions" class="send-options">
          <div class="option-row">
            <label class="option-item">
              换行
              <SelfSelect
                v-model="sendSettings.newline"
                :full="false"
                :options="[
                  { value: 'none', label: '无' },
                  { value: 'lf', label: '\n' },
                  { value: 'crlf', label: '\r\n' },
                  { value: 'cr', label: '\r' },
                ]"
              />
            </label>
            <label class="check-label option-item">
              <input type="checkbox" v-model="sendSettings.clearAfterSend" />
              发送后清空
            </label>
            <label class="forward-label" title="把本会话收到的数据自动转发到目标会话">
              转发到
              <SelfSelect
                v-model="forwardTarget"
                :full="false"
                :options="[
                  { value: '', label: '不转发' },
                  ...sessions.filter((x) => x.id !== activeSession!.id).map((x) => ({ value: x.id, label: x.name })),
                ]"
              />
            </label>
          </div>

          <div class="quick-cmds">
            <span class="quick-title">快捷命令（点击发送到当前会话）</span>
            <div v-for="(cmd, index) in quickCommands" :key="index" class="quick-cmd">
              <input v-model="cmd.name" class="qc-name" placeholder="名称" />
              <input v-model="cmd.text" class="qc-text" placeholder="内容" @keyup.enter="sendQuickCommand(cmd)" />
              <button class="ghost-btn qc-send" :disabled="!connected" @click="sendQuickCommand(cmd)">发送</button>
              <button class="danger-text qc-del" title="删除命令" @click="removeQuickCommand(index)">×</button>
            </div>
            <button class="ghost-btn qc-add" @click="addQuickCommand">+ 添加命令</button>
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

.detail-title p {
  margin: 4px 0 0;
  font-size: 13px;
  color: #6b7280;
}

.button-group {
  display: flex;
  align-items: center;
  gap: 12px;
}

/* 收发合一面板：接收区在上，发送栏在下 */
.work-panel {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
  background: rgba(243, 244, 246, 0.7);
  border: 1px solid rgba(23, 26, 33, 0.07);
  border-radius: 8px;
  padding: 12px;
}

/* 工具带：位于接收框与发送框之间 */
.panel-toolbar {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
  padding: 6px 2px;
  border-top: 1px dashed rgba(23, 26, 33, 0.12);
  flex-shrink: 0;
}

.panel-toolbar .check-label {
  white-space: nowrap;
}

.toolbar-option {
  margin-left: auto;
}

/* 发送栏：输入框 + 自动发送 + 发送按钮（右下角框外）；窄窗口放不下时按钮换行，不产生横向溢出 */
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

/* 发送选项折叠面板 */
.send-options {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px;
  background: rgba(23, 26, 33, 0.03);
  border: 1px dashed rgba(23, 26, 33, 0.14);
  border-radius: 8px;
  flex-shrink: 0;
}

.option-row {
  display: flex;
  align-items: center;
  gap: 14px;
  flex-wrap: wrap;
}

.option-item {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.option-item select {
  width: auto;
  padding-right: 28px;
}


.forward-label {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  margin-left: auto;
}

/* 快捷命令区 */
.quick-cmds {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.quick-title {
  font-size: 12px;
  color: #8a9099;
}

.quick-cmd {
  display: grid;
  grid-template-columns: 88px 1fr auto auto;
  gap: 6px;
  align-items: center;
}

.qc-name,
.qc-text {
  padding: 6px 8px;
}

.qc-send {
  padding: 6px 12px;
}

.qc-del {
  width: 26px;
  padding: 2px 0;
  font-size: 14px;
}

.qc-add {
  align-self: flex-start;
  padding: 5px 10px;
}

/* 深色主题 */
.theme-dark .detail-title p,
.theme-dark .forward-label,
.theme-dark .auto-send {
  color: #9da0a8;
}

.theme-dark .work-panel {
  background: rgba(255, 255, 255, 0.04);
  border-color: rgba(255, 255, 255, 0.09);
}

.theme-dark .send-options {
  background: rgba(255, 255, 255, 0.03);
  border-color: rgba(255, 255, 255, 0.12);
}

.theme-dark .quick-title {
  color: #7c828c;
}
</style>
