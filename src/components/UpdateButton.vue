<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { appliedTheme } from '../stores/appStore';
import { checkUpdate, openReleasePage, type UpdateInfo } from '../utils/update';

// 版本更新按钮：仅检测到新版本时显示（绿色「更新」）。
// 悬停显示更新日志浮层（上边框紧贴按钮，样式与新建会话的下拉浮层一致）；
// 点击打开确认弹窗（再次确认后打开下载页）。
const info = ref<UpdateInfo | null>(null);
const confirmOpen = ref(false);
const updating = ref(false);

// 启动后延迟检测（不阻塞应用启动与其他请求）
onMounted(() => {
  setTimeout(async () => {
    info.value = await checkUpdate();
  }, 3000);
});

// 确认更新：打开 Release 下载页
const doUpdate = async () => {
  if (!info.value || updating.value) return;
  updating.value = true;
  try {
    await openReleasePage(info.value.releaseUrl);
    confirmOpen.value = false;
  } finally {
    updating.value = false;
  }
};
</script>

<template>
  <div v-if="info?.hasUpdate" class="update-wrap" @click.stop>
    <button class="update-btn" title="发现新版本" @click="confirmOpen = true">更新</button>

    <!-- 悬停浮层：上边框紧贴按钮下缘（top:100% 无间距） -->
    <div class="update-pop">
      <p class="up-title">v{{ info.latestVersion }} 更新内容</p>
      <pre class="up-log">{{ info.changelog }}</pre>
      <p class="up-hint">点击「更新」按钮查看确认</p>
    </div>
  </div>

  <!-- 确认弹窗：再次确认后打开下载页 -->
  <Teleport to="body">
    <div v-if="confirmOpen && info" class="up-mask" @click="confirmOpen = false">
      <div class="up-dialog" :class="{ 'theme-dark': appliedTheme === 'dark' }" @click.stop>
        <h3 class="up-dialog-title">确认更新</h3>
        <p class="up-dialog-sub">当前版本 v{{ info.currentVersion }} → 最新版本 v{{ info.latestVersion }}</p>
        <div class="up-dialog-body">
          <p class="up-title">更新内容</p>
          <pre class="up-log">{{ info.changelog }}</pre>
        </div>
        <div class="up-actions">
          <button class="ghost-btn" @click="confirmOpen = false">取消</button>
          <button class="primary-btn" :disabled="updating" @click="doUpdate">
            {{ updating ? '打开中…' : '确认更新' }}
          </button>
        </div>
        <p class="up-dialog-note">确认后将打开下载页面，下载新版本安装包覆盖安装即可完成更新。</p>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.update-wrap {
  position: relative;
}

/* 绿色更新按钮 */
.update-btn {
  padding: 6px 14px;
  background: linear-gradient(180deg, #2eb85c, #28a745);
  color: #ffffff;
  box-shadow: 0 4px 12px rgba(40, 167, 69, 0.3);
}

.update-btn:hover:not(:disabled) {
  background: linear-gradient(180deg, #28a745, #218838);
}

/* 悬停浮层：紧贴按钮下缘（与新建会话下拉浮层同款样式） */
.update-pop {
  position: absolute;
  /* 上边框紧贴按钮下边框（间距 0） */
  top: 100%;
  right: 0;
  z-index: 80;
  width: 280px;
  max-height: 320px;
  overflow: auto;
  padding: 10px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  background: #ffffff;
  border: 1px solid rgba(23, 26, 33, 0.12);
  border-top-left-radius: 0;
  border-radius: 8px;
  box-shadow: 0 12px 32px rgba(15, 23, 42, 0.18);
  opacity: 0;
  visibility: hidden;
  transition: opacity 0.16s ease, visibility 0.16s ease;
}

.update-wrap:hover .update-pop {
  opacity: 1;
  visibility: visible;
}

.up-title {
  margin: 0;
  font-size: 12px;
  font-weight: 700;
  color: #2e8b45;
}

.up-log {
  margin: 0;
  font-family: Consolas, 'Courier New', monospace;
  font-size: 12px;
  line-height: 1.6;
  color: #23262b;
  white-space: pre-wrap;
  word-break: break-word;
}

.up-hint {
  margin: 0;
  font-size: 11px;
  color: #8a9099;
}

/* 深色主题 */
.theme-dark .update-pop {
  background: #33353a;
  border-color: rgba(255, 255, 255, 0.12);
  box-shadow: 0 12px 36px rgba(0, 0, 0, 0.45);
}

.theme-dark .up-log {
  color: #dfdfe3;
}

.theme-dark .up-hint {
  color: #7c828c;
}
</style>

<style scoped>
/* ===== 确认弹窗（Teleport 到 body，需自持主题类） ===== */
.up-mask {
  position: fixed;
  inset: 0;
  z-index: 200;
  display: grid;
  place-items: center;
  background: rgba(0, 0, 0, 0.45);
}

.up-dialog {
  width: 420px;
  max-width: calc(100vw - 48px);
  max-height: calc(100vh - 96px);
  overflow: auto;
  padding: 18px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  background: #ffffff;
  border: 1px solid rgba(23, 26, 33, 0.12);
  border-radius: 10px;
  box-shadow: 0 24px 64px rgba(15, 23, 42, 0.3);
}

.up-dialog.theme-dark {
  background: #2b2d30;
  border-color: rgba(255, 255, 255, 0.1);
}

.up-dialog-title {
  margin: 0;
  font-size: 16px;
  color: #23262b;
}

.up-dialog.theme-dark .up-dialog-title {
  color: #f4f4f6;
}

.up-dialog-sub {
  margin: 0;
  font-size: 13px;
  color: #6b7280;
}

.up-dialog.theme-dark .up-dialog-sub {
  color: #9da0a8;
}

.up-dialog-body {
  padding: 10px;
  border: 1px solid rgba(23, 26, 33, 0.1);
  border-radius: 8px;
  background: rgba(248, 249, 251, 0.8);
  max-height: 240px;
  overflow: auto;
}

.up-dialog.theme-dark .up-dialog-body {
  background: #1e1f22;
  border-color: rgba(255, 255, 255, 0.1);
}

.up-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}

.up-dialog-note {
  margin: 0;
  font-size: 12px;
  color: #8a9099;
}
</style>
