<script setup lang="ts">
import { computed, ref } from 'vue';
import SelfSelect from './SelfSelect.vue';
import { activeForwardRule, clearForwardLogs, forwardLogs, sessions, startForward, stopForward } from '../stores/appStore';

// 转发视图：单规则模型——选好"从哪个会话到哪个会话"，点"开启转发"生效；
// 转发中按钮变为"关闭转发"，点击即停止。下方实时记录每条转发的时间、路径与内容。
const fromId = ref('');
const toId = ref('');

const sessionOptions = computed(() => sessions.value.map((s) => ({ value: s.id, label: s.name })));
const fromOptions = computed(() => [{ value: '', label: '选择来源会话' }, ...sessionOptions.value]);
const toOptions = computed(() => [{ value: '', label: '选择目标会话' }, ...sessionOptions.value]);

// 当前生效中的转发规则
const activeRule = activeForwardRule;
const ruleLabel = computed(() => {
  if (!activeRule.value) return '';
  const from = sessions.value.find((s) => s.id === activeRule.value.fromId)?.name ?? '';
  const to = sessions.value.find((s) => s.id === activeRule.value.toId)?.name ?? '';
  return `转发中：${from} → ${to}`;
});

const onToggle = () => {
  if (activeRule.value) {
    stopForward();
  } else if (fromId.value && toId.value) {
    startForward(fromId.value, toId.value);
  }
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
      <button
        class="primary-btn"
        :class="{ 'stop-btn': activeRule }"
        :disabled="!activeRule && (!fromId || !toId || fromId === toId)"
        :title="activeRule ? ruleLabel : '按所选的来源与目标开启转发'"
        @click="onToggle"
      >
        {{ activeRule ? '关闭转发' : '开启转发' }}
      </button>
      <span v-if="activeRule" class="rule-status">{{ ruleLabel }}</span>
      <button class="ghost-btn log-clear" title="清空下方转发记录" @click="clearForwardLogs">清空</button>
    </div>

    <!-- 转发记录：每条转发的时间、路径与数据内容 -->
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

/* 关闭转发状态：按钮转红以示停止语义 */
.stop-btn {
  background: linear-gradient(180deg, #c74541, #b93b37);
}

.rule-status {
  font-size: 13px;
  color: #2e8b45;
  font-weight: 600;
}

/* 清空按钮：推到行最右端 */
.log-clear {
  margin-left: auto;
}

/* 转发记录：时间 + 路径 + 数据内容，等宽字体滚动列表 */
.forward-log {
  flex: 1;
  min-height: 160px;
  overflow: auto;
  border: 1px solid rgba(23, 26, 33, 0.1);
  border-radius: 6px;
  background: #f8f9fb;
  padding: 8px;
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

.theme-dark .rule-status {
  color: #6bc97e;
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
