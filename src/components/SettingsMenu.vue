<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue';
import SelfSelect from './SelfSelect.vue';
import { fontFamily, fontSize, saveLog, themeMode } from '../stores/appStore';

// 设置菜单：主题三态 + 字号/字体 + 日志落盘 + 时间戳开关。
// 点击菜单外部自动收起（菜单内部点击通过 @click.stop 阻止冒泡）。
const settingsOpen = ref(false);

const onDocClick = () => {
  settingsOpen.value = false;
};

onMounted(() => document.addEventListener('click', onDocClick));
onUnmounted(() => document.removeEventListener('click', onDocClick));
</script>

<template>
  <div class="settings-wrap" @click.stop>
    <button class="icon-btn" title="设置" @click="settingsOpen = !settingsOpen">
      <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="12" cy="12" r="3"></circle>
        <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"></path>
      </svg>
    </button>

    <div v-if="settingsOpen" class="settings-menu">
      <div class="settings-group">
        <span class="settings-label">主题</span>
        <SelfSelect
          v-model="themeMode"
          :full="false"
          :options="[
            { value: 'light', label: '浅色' },
            { value: 'dark', label: '深色' },
            { value: 'system', label: '跟随系统' },
          ]"
        />
      </div>
      <div class="settings-group">
        <span class="settings-label">消息字号</span>
        <div class="mode-row">
          <button v-for="size in [12, 13, 14, 16, 18]" :key="size" :class="{ on: fontSize === size }" @click="fontSize = size">
            {{ size }}
          </button>
        </div>
      </div>
      <label class="switch-row">
        <span>字体</span>
        <SelfSelect
          v-model="fontFamily"
          :full="false"
          :options="[
            { value: 'consolas', label: 'Consolas' },
            { value: 'mono', label: 'Cascadia Mono' },
            { value: 'system', label: '系统字体' },
          ]"
        />
      </label>
      <label class="switch-row">
        <span>保存日志到 log 目录</span>
        <input type="checkbox" v-model="saveLog" />
      </label>
      <p class="settings-hint">提示：时间戳与 RX/TX 标签在每个会话的接收区单独开关；接收区按住 Ctrl + 滚轮可实时缩放字号</p>
    </div>
  </div>
</template>

<style scoped>
.settings-wrap {
  position: relative;
}

.icon-btn {
  width: 32px;
  height: 32px;
  padding: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: rgba(23, 26, 33, 0.05);
  color: #4b5563;
  box-shadow: inset 0 0 0 1px rgba(23, 26, 33, 0.1);
}

.settings-menu {
  position: absolute;
  top: calc(100% + 8px);
  right: 0;
  z-index: 60;
  min-width: 220px;
  padding: 10px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  background: #ffffff;
  border: 1px solid rgba(23, 26, 33, 0.12);
  border-radius: 8px;
  box-shadow: 0 12px 32px rgba(15, 23, 42, 0.14);
}

.settings-group {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.settings-label {
  font-size: 12px;
  color: #8a9099;
}

.mode-row {
  display: flex;
  gap: 6px;
}

.mode-row button {
  flex: 1;
  padding: 6px 0;
  background: rgba(23, 26, 33, 0.05);
  color: #4b5563;
  box-shadow: inset 0 0 0 1px rgba(23, 26, 33, 0.1);
}

.mode-row button.on {
  background: linear-gradient(180deg, #3b6fd4, #3563c2);
  color: #ffffff;
  box-shadow: none;
}

.switch-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  font-size: 13px;
  color: #3b414b;
  cursor: pointer;
}

.font-select {
  width: 150px;
}

.settings-hint {
  font-size: 11px;
  color: #8a9099;
  margin: 2px 0 0;
}

/* 深色主题 */
.theme-dark .icon-btn {
  background: rgba(255, 255, 255, 0.06);
  color: #b3b7be;
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.12);
}

.theme-dark .settings-menu {
  background: #33353a;
  border-color: rgba(255, 255, 255, 0.12);
  box-shadow: 0 12px 36px rgba(0, 0, 0, 0.45);
}

.theme-dark .settings-label {
  color: #7c828c;
}

.theme-dark .mode-row button {
  background: rgba(255, 255, 255, 0.06);
  color: #b3b7be;
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.1);
}

.theme-dark .mode-row button.on {
  background: #3658a7;
  color: #ffffff;
  box-shadow: none;
}

.theme-dark .switch-row {
  color: #c6c9cf;
}
</style>
