<script setup lang="ts">
import { computed, ref } from 'vue';
import SelfSelect from './SelfSelect.vue';
import { addForwardRule, forwardRules, removeForwardRule, sessions } from '../stores/appStore';

// 转发视图：配置"从哪个会话转发到哪个会话"（仅限连接列表中已有的会话）。
// 规则命中时，来源会话收到的数据原样发往目标会话（目标须已连接）。
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
      <button class="primary-btn" :disabled="!fromId || !toId || fromId === toId" @click="addRule">添加规则</button>
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
  align-items: end;
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

.arrow {
  color: #8a9099;
  font-size: 16px;
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
</style>
