<script setup lang="ts">
import { computed, onMounted, ref, shallowRef } from 'vue';
import { appliedTheme } from '../stores/appStore';
import { checkUpdate, openReleasePage, type UpdateInfo } from '../utils/update';
import { check, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

// 版本更新：入口在标题栏应用名右侧（绿色「更新」徽标，仅检测到新版本时显示）。
// 悬停徽标 → 简易更新日志浮层；点击浮层 → 界面中央的详细更新卡片（右上角可关闭，
// 日志下方「更新」按钮执行原地下载安装并自动重启）。
// 浏览器调试环境（无 updater 插件）回退：GitHub API 检测 + 打开下载页。
// 必须用 shallowRef：ref 会对值做深度响应式代理，插件 Update 类的私有字段
// 经代理访问会抛 "Cannot read private member from an object whose class did not declare it"
const inplace = shallowRef<Update | null>(null); // 原地更新模式：插件检测结果
const info = ref<UpdateInfo | null>(null); // 回退模式：GitHub API 检测结果
const detailOpen = ref(false);
const phase = ref<'idle' | 'download' | 'install'>('idle');
const progress = ref(0);
const errMsg = ref('');

const hasUpdate = computed(() => !!inplace.value || !!info.value?.hasUpdate);
const latestVersion = computed(() => inplace.value?.version ?? info.value?.latestVersion ?? '');
const currentVersion = computed(() => inplace.value?.currentVersion ?? info.value?.currentVersion ?? '');
const changelog = computed(() => inplace.value?.body ?? info.value?.changelog ?? '');
const isInstalling = computed(() => phase.value !== 'idle');

// 简易日志：取前 5 行，超出部分提示点开详情
const simpleLog = computed(() => {
  const lines = changelog.value.split('\n').filter((l) => l.trim());
  const head = lines.slice(0, 5).join('\n');
  return lines.length > 5 ? `${head}\n… 点击查看完整更新日志` : head;
});

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

// 详细卡片里的更新按钮：原地下载安装 + 自动重启；回退模式打开下载页
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
    detailOpen.value = false;
  } catch {
    /* 打开浏览器失败忽略 */
  }
};
</script>

<template>
  <div v-if="hasUpdate" class="update-wrap" @click.stop>
    <button class="update-btn" title="发现新版本" @click="detailOpen = true">更新</button>

    <!-- 悬停简易日志浮层：点击打开中央详细卡片 -->
    <div class="update-pop" @click="detailOpen = true">
      <p class="up-title">发现新版本 v{{ latestVersion }}</p>
      <pre class="up-log">{{ simpleLog }}</pre>
      <p class="up-hint">点击查看完整更新日志</p>
    </div>
  </div>

  <!-- 中央详细更新卡片（右上角关闭 X；日志下方为更新执行按钮） -->
  <Teleport to="body">
    <div v-if="detailOpen && hasUpdate" class="up-mask" @click="detailOpen = false">
      <div class="up-card" :class="{ 'theme-dark': appliedTheme === 'dark' }" @click.stop>
        <button class="up-close" title="关闭" @click="detailOpen = false">
          <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
            <line x1="18" y1="6" x2="6" y2="18"></line>
            <line x1="6" y1="6" x2="18" y2="18"></line>
          </svg>
        </button>
        <h3 class="up-card-title">发现新版本</h3>
        <p class="up-card-sub">当前版本 v{{ currentVersion }} → 最新版本 v{{ latestVersion }}</p>
        <p class="up-card-label">更新内容</p>
        <div class="up-card-log">
          <pre class="up-log">{{ changelog }}</pre>
        </div>
        <div v-if="phase === 'download'" class="up-progress">
          <div class="up-progress-bar" :style="{ width: progress + '%' }"></div>
          <span class="up-progress-text">{{ progress }}%</span>
        </div>
        <p v-if="errMsg" class="up-card-err">{{ errMsg }}</p>
        <button class="up-card-btn" :disabled="isInstalling" @click="doUpdate">
          {{ phase === 'download' ? `下载中 ${progress}%` : phase === 'install' ? '安装中…' : inplace ? '更新' : '打开下载页' }}
        </button>
        <p class="up-card-note">
          {{ inplace ? '更新将自动下载并静默安装，完成后软件自动重启进入新版本。' : '将打开下载页面，下载新版本安装包覆盖安装即可完成更新。' }}
        </p>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.update-wrap {
  position: relative;
  margin-left: 10px;
}

/* 绿色更新徽标（标题栏内，与界面文字相比更醒目） */
.update-btn {
  padding: 2px 10px;
  font-size: 11px;
  border-radius: 999px;
  background: linear-gradient(180deg, #2eb85c, #28a745);
  color: #ffffff;
  box-shadow: 0 2px 8px rgba(40, 167, 69, 0.35);
}

.update-btn:hover:not(:disabled) {
  background: linear-gradient(180deg, #28a745, #218838);
}

/* 悬停简易日志浮层：紧贴徽标下方，点击打开详细卡片 */
.update-pop {
  position: absolute;
  top: calc(100% + 8px);
  left: 0;
  z-index: 90;
  width: 300px;
  padding: 10px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  background: #ffffff;
  border: 1px solid rgba(23, 26, 33, 0.12);
  border-radius: 8px;
  box-shadow: 0 12px 32px rgba(15, 23, 42, 0.18);
  cursor: pointer;
}

.update-pop:hover {
  border-color: rgba(59, 111, 212, 0.4);
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
  font-family: inherit;
}

.up-hint {
  margin: 0;
  font-size: 11px;
  color: #8a9099;
}

/* 深色主题（标题栏浮层） */
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

/* ===== 中央详细更新卡片（Teleport 到 body，自持主题类） ===== */
.up-mask {
  position: fixed;
  inset: 0;
  z-index: 200;
  display: grid;
  place-items: center;
  background: rgba(0, 0, 0, 0.45);
}

.up-card {
  position: relative;
  width: 460px;
  max-width: calc(100vw - 48px);
  max-height: calc(100vh - 96px);
  overflow: auto;
  padding: 18px 20px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  background: #ffffff;
  border: 1px solid rgba(23, 26, 33, 0.12);
  border-radius: 10px;
  box-shadow: 0 24px 64px rgba(15, 23, 42, 0.3);
}

.up-card.theme-dark {
  background: #2b2d30;
  border-color: rgba(255, 255, 255, 0.1);
}

/* 右上角关闭图标 */
.up-close {
  position: absolute;
  top: 10px;
  right: 10px;
  width: 26px;
  height: 26px;
  padding: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 5px;
  background: transparent;
  color: #6b7280;
  box-shadow: none;
}

.up-close:hover {
  background: rgba(199, 69, 65, 0.1);
  color: #c74541;
}

.up-card-title {
  margin: 0;
  font-size: 16px;
  color: #23262b;
}

.up-card.theme-dark .up-card-title {
  color: #f4f4f6;
}

.up-card-sub {
  margin: 0;
  font-size: 13px;
  color: #6b7280;
}

.up-card.theme-dark .up-card-sub {
  color: #9da0a8;
}

.up-card-label {
  margin: 0;
  font-size: 12px;
  font-weight: 700;
  color: #2e8b45;
}

.up-card-log {
  padding: 10px;
  border: 1px solid rgba(23, 26, 33, 0.1);
  border-radius: 8px;
  background: rgba(248, 249, 251, 0.8);
  max-height: 240px;
  overflow: auto;
}

.up-card.theme-dark .up-card-log {
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

/* 日志下方的更新执行按钮 */
.up-card-btn {
  padding: 9px 16px;
  background: linear-gradient(180deg, #2eb85c, #28a745);
  color: #ffffff;
  box-shadow: 0 4px 12px rgba(40, 167, 69, 0.3);
}

.up-card-btn:hover:not(:disabled) {
  background: linear-gradient(180deg, #28a745, #218838);
}

.up-card-btn:disabled {
  opacity: 0.7;
}

.up-card-note {
  margin: 0;
  font-size: 12px;
  color: #8a9099;
}

.up-card-err {
  margin: 0;
  font-size: 12px;
  color: #c74541;
  word-break: break-all;
}
</style>
