<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { appliedTheme } from '../stores/appStore';
import { checkUpdate, openReleasePage, type UpdateInfo } from '../utils/update';
import { check, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

// 版本更新按钮：仅检测到新版本时显示（绿色「更新」）。
// 桌面环境优先用 updater 插件做原地更新（读取 latest.json，签名校验，
// 下载 → 静默安装 → 自动重启）；插件不可用（浏览器调试）时回退为
// GitHub API 检测 + 打开 Release 下载页。
const inplace = ref<Update | null>(null); // 原地更新模式：插件检测结果
const info = ref<UpdateInfo | null>(null); // 回退模式：GitHub API 检测结果
const confirmOpen = ref(false);
const phase = ref<'idle' | 'download' | 'install'>('idle');
const progress = ref(0);
const errMsg = ref('');

const hasUpdate = computed(() => !!inplace.value || !!info.value?.hasUpdate);
const latestVersion = computed(() => inplace.value?.version ?? info.value?.latestVersion ?? '');
const currentVersion = computed(() => inplace.value?.currentVersion ?? info.value?.currentVersion ?? '');
const changelog = computed(() => inplace.value?.body ?? info.value?.changelog ?? '');
const isInstalling = computed(() => phase.value !== 'idle');

// 启动后延迟检测（不阻塞应用启动）
onMounted(() => {
  setTimeout(async () => {
    try {
      // 插件检测：读取更新端点 latest.json 并做版本比较，无更新返回 null
      inplace.value = (await check()) ?? null;
      if (inplace.value) return;
    } catch {
      /* 浏览器调试环境 / 插件不可用 → 走 GitHub API 回退 */
    }
    info.value = await checkUpdate();
  }, 3000);
});

// 确认更新：原地模式走下载安装 + 自动重启；回退模式打开下载页
const doUpdate = async () => {
  if (isInstalling.value) return;
  if (inplace.value) {
    errMsg.value = '';
    phase.value = 'download';
    progress.value = 0;
    try {
      let total = 0;
      let done = 0;
      await inplace.value.downloadAndInstall((event) => {
        if (event.event === 'Started') {
          total = event.data.contentLength ?? 0;
        } else if (event.event === 'Progress') {
          done += event.data.chunkLength;
          progress.value = total ? Math.min(100, Math.round((done / total) * 100)) : 0;
        } else if (event.event === 'Finished') {
          phase.value = 'install';
        }
      });
      await relaunch(); // 安装完成自动重启进入新版本
    } catch (e) {
      errMsg.value = `更新失败: ${e}`;
      phase.value = 'idle';
    }
    return;
  }
  if (!info.value) return;
  try {
    await openReleasePage(info.value.releaseUrl);
    confirmOpen.value = false;
  } catch {
    /* 打开浏览器失败忽略 */
  }
};
</script>

<template>
  <div v-if="hasUpdate" class="update-wrap" @click.stop>
    <button class="update-btn" title="发现新版本" @click="confirmOpen = true">更新</button>

    <!-- 悬停浮层：上边框紧贴按钮下缘（top:100% 无间距） -->
    <div class="update-pop">
      <p class="up-title">v{{ latestVersion }} 更新内容</p>
      <pre class="up-log">{{ changelog }}</pre>
      <p class="up-hint">点击「更新」按钮查看确认</p>
    </div>
  </div>

  <!-- 确认弹窗 -->
  <Teleport to="body">
    <div v-if="confirmOpen && hasUpdate" class="up-mask" @click="confirmOpen = false">
      <div class="up-dialog" :class="{ 'theme-dark': appliedTheme === 'dark' }" @click.stop>
        <h3 class="up-dialog-title">确认更新</h3>
        <p class="up-dialog-sub">当前版本 v{{ currentVersion }} → 最新版本 v{{ latestVersion }}</p>
        <div class="up-dialog-body">
          <p class="up-title">更新内容</p>
          <pre class="up-log">{{ changelog }}</pre>
        </div>
        <div v-if="phase === 'download'" class="up-progress">
          <div class="up-progress-bar" :style="{ width: progress + '%' }"></div>
          <span class="up-progress-text">{{ progress }}%</span>
        </div>
        <div class="up-actions">
          <button class="ghost-btn" :disabled="isInstalling" @click="confirmOpen = false">取消</button>
          <button class="primary-btn" :disabled="isInstalling" @click="doUpdate">
            {{ phase === 'download' ? `下载中 ${progress}%` : phase === 'install' ? '安装中…' : inplace ? '确认更新' : '打开下载页' }}
          </button>
        </div>
        <p v-if="errMsg" class="up-dialog-err">{{ errMsg }}</p>
        <p class="up-dialog-note">
          {{ inplace ? '确认后将自动下载并静默安装，完成后软件自动重启进入新版本。' : '确认后将打开下载页面，下载新版本安装包覆盖安装即可完成更新。' }}
        </p>
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

/* 下载进度条 */
.up-progress {
  display: flex;
  align-items: center;
  gap: 10px;
}

.up-progress-bar {
  height: 8px;
  flex: 1;
  border-radius: 4px;
  background: linear-gradient(180deg, #2eb85c, #28a745);
  transition: width 0.2s ease;
}

.up-progress-text {
  font-size: 12px;
  color: #2e8b45;
  font-variant-numeric: tabular-nums;
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

.up-dialog-err {
  margin: 0;
  font-size: 12px;
  color: #c74541;
  word-break: break-all;
}
</style>
