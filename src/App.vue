<script setup lang="ts">
import { onMounted, onUnmounted, ref, watchEffect } from 'vue';
import { appliedTheme, disposeApp, fontFamilyStack, initApp, viewMode } from './stores/appStore';
import { getCurrentWindow } from '@tauri-apps/api/window';
import logoUrl from './assets/logo.png';

// ---------- 等比适配窗口 ----------
// 以 1280x800 为基准，取"宽度比"与"高度比"中较小者作为整体缩放：
// 缩小窗口 → UI 等比变小（纵向永远放得下）；拉大 → 等比变大，布局比例恒定。
// 仅保留 0.85 下限，最小窗口下保护可读性。
const uiScale = ref(1);
const updateUiScale = () => {
  uiScale.value = Math.max(0.85, Math.min(window.innerWidth / 1280, window.innerHeight / 800));
};
watchEffect(() => {
  document.documentElement.style.zoom = String(uiScale.value);
});

// 自定义标题栏的窗口控制（无边框窗口）。浏览器调试环境无 Tauri API，静默降级。
// getCurrentWindow 在非 Tauri 环境会抛错，需捕获避免白屏
const appWindow = (() => {
  try {
    return getCurrentWindow();
  } catch {
    return null;
  }
})();
const winMinimize = () => void appWindow?.minimize().catch(() => {});
const winToggleMaximize = () => void appWindow?.toggleMaximize().catch(() => {});
const winClose = () => void appWindow?.close().catch(() => {});
import BusView from './components/BusView.vue';
import DetailView from './components/DetailView.vue';
import ForwardView from './components/ForwardView.vue';
import SessionForm from './components/SessionForm.vue';
import SessionList from './components/SessionList.vue';
import SettingsMenu from './components/SettingsMenu.vue';
import ToastHost from './components/ToastHost.vue';
import UpdateButton from './components/UpdateButton.vue';
import SplitView from './components/SplitView.vue';
import ViewSwitch from './components/ViewSwitch.vue';

// App.vue 只负责布局骨架与生命周期装配；
// 状态与动作集中在 stores/appStore.ts，界面拆分在 components/ 下。
// 等比缩放随窗口尺寸实时更新：拉宽/拉高 → UI 等比变大；缩小 → 等比变小。
onMounted(() => {
  window.addEventListener('resize', updateUiScale);
  updateUiScale();
  void initApp();
});

onUnmounted(() => {
  window.removeEventListener('resize', updateUiScale);
  disposeApp();
});
</script>

<template>
  <main class="workspace" :class="`theme-${appliedTheme}`" :style="{ fontFamily: fontFamilyStack }">
    <!-- 自定义标题栏（ZCode 风格）：左 logo+名称，右侧窗口控制；空白区可拖动窗口 -->
    <div class="titlebar">
      <div class="tb-left" data-tauri-drag-region>
        <img :src="logoUrl" class="tb-logo-img" alt="NNSerialTool" draggable="false" />
        <span class="tb-title">NNSerialTool</span>
        <!-- 更新入口：仅检测到新版本时显示（悬停简易日志，点开中央详细卡片） -->
        <UpdateButton />
      </div>
      <div class="tb-drag" data-tauri-drag-region></div>
      <div class="tb-controls">
        <button class="tb-btn" title="最小化" @click="winMinimize">
          <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><line x1="5" y1="12" x2="19" y2="12"></line></svg>
        </button>
        <button class="tb-btn" title="最大化 / 还原" @click="winToggleMaximize">
          <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.6"><rect x="6" y="6" width="12" height="12" rx="1.5"></rect></svg>
        </button>
        <button class="tb-btn tb-close" title="关闭" @click="winClose">
          <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><line x1="6" y1="6" x2="18" y2="18"></line><line x1="18" y1="6" x2="6" y2="18"></line></svg>
        </button>
      </div>
    </div>
    <div class="app-body">
    <aside class="sidebar">
      <SessionForm />
      <SessionList />
    </aside>

    <section class="main-area">
      <header class="toolbar">
        <div>
          <h1>工作台</h1>
        </div>
        <div class="toolbar-actions">
          <SettingsMenu />
          <ViewSwitch />
        </div>
      </header>

      <DetailView v-if="viewMode === 'detail'" />
      <SplitView v-else-if="viewMode === 'split'" />
      <ForwardView v-else-if="viewMode === 'forward'" />
      <BusView v-else />
    </section>
    </div>
    <!-- 中央通知（INFO 提示上浮消失） -->
    <ToastHost />
  </main>
</template>
