<script setup lang="ts">
import { computed, ref } from 'vue';
import { appliedTheme } from '../stores/appStore';
import {
  changelog,
  currentVersion,
  detailOpen,
  hasUpdate,
  historyLoading,
  inplaceUpdate,
  latestVersion,
  openDetailCard,
  versionHistory,
} from '../stores/updateStore';
import { groupReleaseNotes, openReleasePage, type ReleaseGroup } from '../utils/update';
import { relaunch } from '@tauri-apps/plugin-process';

// 版本更新：入口在标题栏应用名右侧（绿色「更新」徽标，仅检测到新版本时显示）。
// 悬停徽标 → 简易更新日志浮层；点击浮层 → 界面中央的详细更新卡片（右上角可关闭，
// 日志下方「更新」按钮执行原地下载安装并自动重启）。
// 检测逻辑与"设置-检查更新"共用 updateStore。

const phase = ref<'idle' | 'download' | 'install'>('idle');
const progress = ref(0);
const errMsg = ref('');
// 下载取消标记：置位后忽略下载事件、完成后不安装不重启
let cancelled = false;

const isInstalling = computed(() => phase.value !== 'idle');

// 简易日志：取前 5 行，超出部分提示点开详情
const simpleLog = computed(() => {
  const lines = changelog.value.split('\n').filter((l) => l.trim());
  const head = lines.slice(0, 5).join('\n');
  return lines.length > 5 ? `${head}\n… 点击查看完整更新日志` : head;
});

// 详细卡片的分版本视图：优先展示"当前版本 → 最新版本"的每个 Release；
// 历史拉取失败时回退为单条最新版说明（同样走归类解析）。
// 每条说明解析成 新功能/优化/修复/文档 分类小节，没归上类的原文显示。
const displayEntries = computed<Array<{ version: string; date: string; groups: ReleaseGroup[]; raw: string }>>(() => {
  if (versionHistory.value.length > 0) {
    return versionHistory.value.map((entry) => ({
      version: entry.version,
      date: entry.date,
      groups: groupReleaseNotes(entry.notes),
      raw: entry.notes,
    }));
  }
  const notes = changelog.value;
  return notes ? [{ version: '', date: '', groups: groupReleaseNotes(notes), raw: notes }] : [];
});

// 下载完成后安装并重启
const installAndRestart = async () => {
  phase.value = 'install';
  try {
    await inplaceUpdate.value?.install();
    // Windows：安装器启动后应用自动退出；macOS/Linux 需手动重启
    await relaunch();
  } catch (e) {
    errMsg.value = `安装失败: ${e}`;
    phase.value = 'idle';
  }
};

// 详细卡片里的更新按钮：桌面原地下载（完成后自动安装重启）；浏览器回退打开下载页
const doUpdate = async () => {
  if (isInstalling.value) return;
  if (!inplaceUpdate.value) {
    // 浏览器调试环境回退：打开 Release 下载页
    try {
      await openReleasePage(`https://github.com/lsx666xsl/NNSerialTool/releases/latest`);
    } catch {
      /* 打开浏览器失败忽略 */
    }
    return;
  }
  errMsg.value = '';
  cancelled = false;
  phase.value = 'download';
  progress.value = 0;
  try {
    let total = 0;
    let done = 0;
    await inplaceUpdate.value.download((event) => {
      if (cancelled) return; // 已取消：忽略后续进度事件
      if (event.event === 'Started') {
        total = event.data.contentLength ?? 0;
      } else if (event.event === 'Progress') {
        done += event.data.chunkLength;
        progress.value = total ? Math.min(99, Math.round((done / total) * 100)) : 0;
      }
    });
    if (cancelled) {
      // 取消发生在下载完成之后：释放已下载数据，不安装
      try {
        await inplaceUpdate.value.close();
      } catch {
        /* 资源可能已释放 */
      }
      return;
    }
    progress.value = 100;
    await installAndRestart();
  } catch (e) {
    if (!cancelled) {
      errMsg.value = `下载失败: ${e}`;
      phase.value = 'idle';
    }
  }
};

// 取消更新：停止安装流程并释放已下载的数据（如有）
const cancelUpdate = async () => {
  if (phase.value === 'install') return; // 安装已启动，无法回退
  cancelled = true;
  phase.value = 'idle';
  detailOpen.value = false;
  try {
    await inplaceUpdate.value?.close(); // 释放已下载的字节资源
  } catch {
    /* 资源可能已释放 */
  }
  inplaceUpdate.value = null;
  notifyCancelled();
};

const notifyCancelled = () => {
  // 通过 appStore 的 notify 提示取消结果（避免循环依赖，动态引入）
  void import('../stores/appStore').then(({ notify }) => notify('更新已取消，已清理下载内容'));
};
</script>

<template>
  <div v-if="hasUpdate" class="update-wrap" @click.stop>
    <button class="update-btn" title="发现新版本" @click="openDetailCard()">更新</button>

    <!-- 悬停简易日志浮层：点击打开中央详细卡片；详细卡片打开期间隐藏 -->
    <div v-show="!detailOpen" class="update-pop" @click="openDetailCard()">
      <p class="up-title">发现新版本 v{{ latestVersion }}</p>
      <pre class="up-log">{{ simpleLog }}</pre>
      <p class="up-hint">点击查看完整更新日志</p>
    </div>
  </div>

  <!-- 中央详细更新卡片；下载中仅右上角 X 可取消，点击遮罩不关闭 -->
  <Teleport to="body">
    <div
      v-if="detailOpen && hasUpdate"
      class="up-mask"
      @click="phase === 'idle' && (detailOpen = false)"
    >
      <div class="up-card" :class="{ 'theme-dark': appliedTheme === 'dark' }" @click.stop>
        <button
          class="up-close"
          :class="{ 'up-close-cancel': phase === 'download' }"
          :title="phase === 'download' ? '取消更新并清理已下载内容' : '关闭'"
          @click="phase === 'download' ? cancelUpdate() : (detailOpen = false)"
        >
          <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
            <line x1="18" y1="6" x2="6" y2="18"></line>
            <line x1="6" y1="6" x2="18" y2="18"></line>
          </svg>
        </button>
        <h3 class="up-card-title">发现新版本</h3>
        <p class="up-card-sub">当前版本 v{{ currentVersion }} → 最新版本 v{{ latestVersion }}</p>
        <p class="up-card-label">{{ phase === 'download' ? '正在下载更新' : '更新内容' }}</p>
        <div class="up-card-log" :class="{ downloading: phase === 'download' }">
          <pre v-if="phase === 'download'" class="up-log">正在从更新源下载新版本安装包，完成后将自动安装并重启…</pre>
          <p v-else-if="historyLoading" class="up-history-loading">正在获取分版本更新日志…</p>
          <div v-else-if="displayEntries.length" class="up-history">
            <section v-for="(entry, idx) in displayEntries" :key="entry.version || `latest-${idx}`" class="up-ver">
              <div v-if="entry.version" class="up-ver-head">
                <span class="up-ver-name">v{{ entry.version }}</span>
                <span class="up-ver-date">{{ entry.date }}</span>
              </div>
              <!-- 分类别小节：只显示有内容的分类，没有则整版回退原文 -->
              <div v-if="entry.groups.length" class="up-ver-body">
                <div v-for="group in entry.groups" :key="group.kind" class="up-group">
                  <span class="up-group-tag" :class="`tag-${group.kind}`">{{ group.label }}</span>
                  <ul class="up-group-items">
                    <li v-for="(item, i) in group.items" :key="i">{{ item }}</li>
                  </ul>
                </div>
              </div>
              <pre v-else class="up-log">{{ entry.raw || '（该版本未填写更新说明）' }}</pre>
            </section>
          </div>
          <pre v-else class="up-log">（本次发布未填写更新说明）</pre>
        </div>
        <div v-if="phase === 'download'" class="up-progress">
          <div class="up-progress-bar" :style="{ width: progress + '%' }"></div>
          <span class="up-progress-text">{{ progress }}%</span>
        </div>
        <p v-if="errMsg" class="up-card-err">{{ errMsg }}</p>
        <button v-if="phase === 'idle'" class="up-card-btn" @click="doUpdate">
          {{ inplaceUpdate ? '更新' : '打开下载页' }}
        </button>
        <p class="up-card-note">
          {{ phase === 'download' ? '下载中请保持软件开启；点击右上角 × 可取消并清理已下载内容。' : inplaceUpdate ? '点击「更新」将自动下载并静默安装，完成后软件自动重启进入新版本。' : '点击「打开下载页」前往手动下载安装。' }}
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
  width: 520px;
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

/* 右上角关闭/取消图标；下载中变为取消语义（红框提示） */
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

.up-close.up-close-cancel {
  color: #c74541;
  box-shadow: inset 0 0 0 1px rgba(199, 69, 65, 0.4);
}

.up-close.up-close-cancel:hover {
  background: rgba(199, 69, 65, 0.15);
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

.up-card.theme-dark .up-card-label {
  color: #6bc97e;
}

.up-card-log {
  padding: 10px;
  border: 1px solid rgba(23, 26, 33, 0.1);
  border-radius: 8px;
  background: rgba(248, 249, 251, 0.8);
  max-height: min(46vh, 420px);
  overflow: auto;
}

/* 分版本更新历史：版本大标题 + 分类小节（新功能/优化/修复…），区域内部滚动 */
.up-history-loading {
  margin: 0;
  padding: 14px 0;
  font-size: 12px;
  color: #8a9099;
  text-align: center;
}

.up-ver + .up-ver {
  margin-top: 4px;
  padding-top: 10px;
  border-top: 1px dashed rgba(23, 26, 33, 0.14);
}

.up-ver-head {
  display: flex;
  align-items: baseline;
  gap: 8px;
}

.up-ver-name {
  font-size: 14px;
  font-weight: 700;
  color: #23262b;
}

.up-ver-date {
  font-size: 11px;
  color: #8a9099;
}

.up-ver-body {
  margin-top: 6px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.up-group {
  display: flex;
  align-items: flex-start;
  gap: 8px;
}

.up-group-tag {
  flex-shrink: 0;
  padding: 0 8px;
  font-size: 11px;
  line-height: 1.7;
  font-weight: 600;
  border-radius: 999px;
  white-space: nowrap;
}

.tag-feature {
  color: #2e8b45;
  background: rgba(46, 139, 69, 0.12);
}

.tag-improve {
  color: #2b6cb0;
  background: rgba(59, 111, 212, 0.12);
}

.tag-fix {
  color: #b7791f;
  background: rgba(183, 121, 31, 0.14);
}

.tag-docs {
  color: #6b7280;
  background: rgba(107, 114, 128, 0.14);
}

.tag-other {
  color: #8a9099;
  background: rgba(138, 144, 153, 0.14);
}

.up-group-items {
  flex: 1;
  margin: 0;
  padding: 0;
  list-style: none;
}

.up-group-items li {
  position: relative;
  padding-left: 12px;
  font-size: 12px;
  line-height: 1.7;
  color: #23262b;
  white-space: pre-wrap;
  word-break: break-word;
}

.up-group-items li::before {
  content: '';
  position: absolute;
  left: 2px;
  top: 0.66em;
  width: 4px;
  height: 4px;
  border-radius: 50%;
  background: rgba(23, 26, 33, 0.35);
}

.up-card.theme-dark .up-card-log {
  background: #1e1f22;
  border-color: rgba(255, 255, 255, 0.1);
}

.theme-dark .up-history-loading {
  color: #7c828c;
}

.theme-dark .up-ver + .up-ver {
  border-top-color: rgba(255, 255, 255, 0.14);
}

.theme-dark .up-ver-name {
  color: #f4f4f6;
}

.theme-dark .up-ver-date {
  color: #7c828c;
}

.theme-dark .tag-feature {
  color: #6bc97e;
  background: rgba(107, 201, 126, 0.16);
}

.theme-dark .tag-improve {
  color: #6ca7e8;
  background: rgba(108, 167, 232, 0.16);
}

.theme-dark .tag-fix {
  color: #d9a441;
  background: rgba(217, 164, 65, 0.16);
}

.theme-dark .tag-docs {
  color: #9da0a8;
  background: rgba(157, 160, 168, 0.16);
}

.theme-dark .tag-other {
  color: #7c828c;
  background: rgba(124, 130, 140, 0.16);
}

.theme-dark .up-group-items li {
  color: #d6d9de;
}

.theme-dark .up-group-items li::before {
  background: rgba(255, 255, 255, 0.3);
}

.up-card-log.downloading {
  border-style: dashed;
}

/* 下载进度条：蓝色斜纹动画（与绿色更新按钮区分） */
.up-progress {
  display: flex;
  align-items: center;
  gap: 10px;
}

.up-progress-bar {
  height: 10px;
  flex: 1;
  border-radius: 5px;
  overflow: hidden;
  background: rgba(59, 130, 246, 0.18);
}

.up-progress-bar::before {
  content: '';
  display: block;
  height: 100%;
  width: var(--up-progress, 0%);
  border-radius: 5px;
  background: linear-gradient(45deg, #3b82f6 25%, #60a5fa 25% 50%, #3b82f6 50% 75%, #60a5fa 75%);
  background-size: 18px 18px;
  animation: up-progress-stripe 0.7s linear infinite;
  transition: width 0.25s ease;
}

@keyframes up-progress-stripe {
  from {
    background-position: 0 0;
  }
  to {
    background-position: 18px 0;
  }
}

.up-progress-text {
  font-size: 12px;
  color: #3b82f6;
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
