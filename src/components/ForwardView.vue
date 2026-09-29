<script setup lang="ts">
import { computed, ref } from 'vue';
import SelfSelect from './SelfSelect.vue';
import { addForwardRule, clearForwardLogs, forwardLogs, forwardRules, removeForwardRule, sessions } from '../stores/appStore';

// 转发视图：配置"从哪个会话转发到哪个会话"（仅限连接列表中已有的会话）。
// 规则命中时，来源会话收到的数据原样发往目标会话（目标须已连接）；
// 下方实时记录每条转发的时间、路径与内容。
const fromId = ref('');
const toId = ref('');

const sessionOptions = computed(() => sessions.value.map((s) => ({ value: s.id, label: s.name })));
const fromOptions = computed(() => [{ value: '', label: '选择来源会话' }, ...sessionOptions.value]);
const toOptions = computed(() => [{ value: '', label: '选择目标会话' }, ...sessionOptions.value]);

const sessionName = (id: string) => sessions.value.find((s) => s.id === id)?.name ?? '（已删除）';

const addRule = () => {
  if (!fromId.value || !toId.value) return;
  addForwardRule(fromId.value, toId.value);
};
</script>

<template>
  <section class="content-card forward-view">
    <div class="forward-bar">
      <label>
        从
        <SelfSelect v-model="fromId" :full="false" :options="fromOptions" />
      </label>
      <span class="arrow">→</span>
      <label>
        到
        <SelfSelect v-model="toId" :full="false" :options="toOptions" />
      </label>
      <button class="primary-btn" :disabled="!fromId || !toId || fromId === toId" @click="addRule">添加</button>
    </div>
    <p class="forward-hint">规则命中后，来源会话收到的数据会原样发往目标会话（目标需已连接）；转发不会再次引发转发，天然无环路。</p>

    <div class="rule-list">
      <div v-if="forwardRules.length === 0" class="empty-box">暂无转发规则，请在上方选择来源与目标会话后添加。</div>
      <div v-for="rule in forwardRules" :key="rule.id" class="rule-line">
        <span class="rule-from">{{ sessionName(rule.fromId) }}</span>
        <span class="rule-arrow">→</span>
        <span class="rule-to">{{ sessionName(rule.toId) }}</span>
        <button class="rule-del" title="删除规则" @click="removeForwardRule(rule.id)">删除</button>
      </div>
    </div>

    <!-- 转发记录：每条转发的时间、路径与数据内容 -->
    <div class="log-head">
      <span class="log-title">转发记录</span>
      <button class="ghost-btn log-clear" @click="clearForwardLogs">清空</button>
    </div>
    <div class="forward-log">
      <div v-if="forwardLogs.length === 0" class="empty-box">暂无转发记录，转发发生时将在此实时显示。</div>
      <div v-for="entry in forwardLogs" :key="entry.id" class="log-line">
        <span class="log-time">{{ entry.time }}</span>
        <span class="log-path">{{ entry.fromName }} → {{ entry.toName }}</span>
        <span class="log-text">{{ entry.text }}</span>
      </div>
    </div>
  </section>
</template>

<style scoped>
.forward-view {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.forward-bar {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}

.forward-bar label {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: #3b414b;
}

/* 来源/目标之间的箭头：与下拉框垂直居中对齐 */
.arrow {
  color: #8a9099;
  font-size: 16px;
  line-height: 1;
  transform: translateY(2px);
}

.forward-hint {
  margin: 0;
  font-size: 12px;
  color: #8a9099;
}

.rule-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.rule-line {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 12px;
  border: 1px solid rgba(23, 26, 33, 0.08);
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.6);
}

.rule-from {
  color: #2e8b45;
  font-weight: 600;
}

.rule-to {
  color: #2b6cb0;
  font-weight: 600;
}

.rule-arrow {
  color: #8a9099;
}

.rule-del {
  margin-left: auto;
  padding: 4px 12px;
  font-size: 12px;
  color: #c74541;
  background: rgba(199, 69, 65, 0.08);
  box-shadow: inset 0 0 0 1px rgba(199, 69, 65, 0.2);
}

/* 转发记录：时间 + 路径 + 数据内容，等宽字体滚动列表 */
.log-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}

.log-title {
  font-size: 13px;
  font-weight: 600;
  color: #3b414b;
}

.log-clear {
  padding: 4px 12px;
  font-size: 12px;
}

.forward-log {
  flex: 1;
  min-height: 160px;
  overflow: auto;
  border: 1px solid rgba(23, 26, 33, 0.1);
  border-radius: 6px;
  background: #f8f9fb;
  padding: 8px;
  font-family: Consolas, 'Courier New', monospace;
  font-size: 12px;
  scrollbar-width: thin;
  scrollbar-color: rgba(128, 132, 140, 0.45) transparent;
}

.forward-log::-webkit-scrollbar {
  width: 8px;
}

.forward-log::-webkit-scrollbar-thumb {
  background: rgba(128, 132, 140, 0.45);
  border-radius: 4px;
}

.log-line {
  display: flex;
  align-items: baseline;
  gap: 10px;
  padding: 3px 4px;
  border-radius: 3px;
  white-space: pre-wrap;
  word-break: break-all;
  line-height: 1.5;
  color: #23262b;
}

.log-line:hover {
  background: rgba(23, 26, 33, 0.04);
}

.log-time {
  flex-shrink: 0;
  color: #7c828c;
}

.log-path {
  flex-shrink: 0;
  color: #2b6cb0;
  font-weight: 600;
}

/* 深色主题 */
.theme-dark .forward-bar label {
  color: #9da0a8;
}

.theme-dark .rule-line {
  background: rgba(255, 255, 255, 0.04);
  border-color: rgba(255, 255, 255, 0.09);
}

.theme-dark .rule-from {
  color: #6bc97e;
}

.theme-dark .rule-to {
  color: #6ca7e8;
}

.theme-dark .rule-del {
  color: #e8807c;
  background: rgba(232, 128, 124, 0.12);
}

.theme-dark .log-title {
  color: #c6c9cf;
}

.theme-dark .forward-log {
  background: #1a1b1e;
  border-color: rgba(255, 255, 255, 0.09);
  color: #d6d9de;
}

.theme-dark .log-line {
  color: #d6d9de;
}

.theme-dark .log-line:hover {
  background: rgba(255, 255, 255, 0.05);
}

.theme-dark .log-time {
  color: #7c828c;
}

.theme-dark .log-path {
  color: #6ca7e8;
}
</style>
