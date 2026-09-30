<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { open } from '@tauri-apps/plugin-dialog';
import SelfSelect from './SelfSelect.vue';
import { defaultLogDir, fontFamily, fontSize, logDir, notify, systemFonts, themeMode } from '../stores/appStore';
import { checking, detailOpen, runUpdateCheck } from '../stores/updateStore';

// 设置菜单：主题三态 + 字号 + 字体 + 日志导出路径（带目录选择器）。
// 每组均为"标签一行、控件另起一行"的纵向布局；点击菜单外部自动收起。
const settingsOpen = ref(false);

// 字体下拉：三个预设项在前，其后追加 DirectWrite 枚举的 Windows 已安装字体
const fontOptions = computed(() => [
  { value: 'consolas', label: 'Consolas' },
  { value: 'mono', label: 'Cascadia Mono' },
  { value: 'system', label: '系统字体' },
  ...systemFonts.value
    .filter((f) => f !== 'Consolas' && f !== 'Cascadia Mono')
    .map((f) => ({ value: f, label: f })),
]);

// 系统目录选择器：选中即写入并持久化；取消则保持原值（留空 = 安装目录 log 默认）
const pickLogDir = async () => {
  try {
    const dir = await open({ directory: true, multiple: false, title: '选择日志导出目录' });
    if (typeof dir === 'string' && dir) {
      logDir.value = dir;
      notify(`日志导出目录已设置：${dir}`);
    }
  } catch {
    /* 浏览器调试环境无原生对话框，静默忽略 */
  }
};

// 检查更新：手动触发一次检测（与标题栏更新徽标共享检测结果）。
// 发现新版本 → 弹出中央详细更新卡片；已是最新 → toast 提示。
const onCheckUpdate = async () => {
  const result = await runUpdateCheck();
  if (result === 'update') {
    detailOpen.value = true;
  } else if (result === 'latest') {
    notify('当前已是最新版本');
  } else {
    notify('检测更新失败，请检查网络后重试');
  }
};

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
      <div class="settings-group">
        <span class="settings-label">字体</span>
        <SelfSelect v-model="fontFamily" :options="fontOptions" />
      </div>
      <div class="settings-group">
        <span class="settings-label">日志导出路径（留空 = 安装目录 log）</span>
        <div class="log-dir-row">
          <input v-model="logDir" class="log-dir-input" :placeholder="defaultLogDir || '安装目录\\log'" spellcheck="false" />
          <button class="dir-pick" title="选择日志导出目录" @click="pickLogDir">…</button>
        </div>
      </div>
      <!-- 检查更新：手动触发一次检测，发现新版本会弹出中央详细更新卡片 -->
      <div class="settings-group check-group">
        <button class="check-btn" :disabled="checking" @click="onCheckUpdate">
          {{ checking ? '检查中…' : '检查更新' }}
        </button>
      </div>
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

.log-dir-row {
  display: flex;
  gap: 6px;
  align-items: stretch;
}

.log-dir-input {
  flex: 1;
  min-width: 0;
  font-size: 12.5px;
  padding: 7px 10px;
}

/* 目录选择按钮：三个点，与输入框同高 */
.dir-pick {
  width: 34px;
  padding: 0;
  font-size: 15px;
  font-weight: 700;
  letter-spacing: 1px;
  background: rgba(23, 26, 33, 0.05);
  color: #4b5563;
  box-shadow: inset 0 0 0 1px rgba(23, 26, 33, 0.1);
}

.dir-pick:hover {
  background: rgba(59, 111, 212, 0.1);
  color: #3563c2;
}

/* 检查更新按钮：设置菜单最底部，通栏 */
.check-group {
  margin-top: 2px;
}

.check-btn {
  width: 100%;
  padding: 8px 0;
  font-size: 13px;
  border-radius: 6px;
  background: rgba(46, 184, 92, 0.1);
  color: #28a745;
  box-shadow: inset 0 0 0 1px rgba(46, 184, 92, 0.3);
}

.check-btn:hover:not(:disabled) {
  background: rgba(46, 184, 92, 0.2);
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

.theme-dark .dir-pick {
  background: rgba(255, 255, 255, 0.06);
  color: #b3b7be;
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.1);
}

.theme-dark .dir-pick:hover {
  background: rgba(87, 157, 245, 0.16);
  color: #8fbdf7;
}

.theme-dark .check-btn {
  background: rgba(46, 184, 92, 0.14);
  color: #6bc97e;
  box-shadow: inset 0 0 0 1px rgba(46, 184, 92, 0.4);
}

.theme-dark .dir-pick:hover {
  background: rgba(87, 157, 245, 0.16);
  color: #8fbdf7;
}
</style>
