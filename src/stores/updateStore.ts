// 更新状态共享模块：标题栏更新徽标与设置里的"检查更新"按钮共用同一份检测状态，
// 保证两处入口看到的结果一致、中央详细卡片只有一个数据源。
// 注意：updater 插件返回的 Update 实例必须用 shallowRef（ref 的深度代理会让
// 插件类的私有字段访问抛 TypeError）。
import { computed, ref, shallowRef } from 'vue';
import { getVersion } from '@tauri-apps/api/app';
import { check, type Update } from '@tauri-apps/plugin-updater';
import { checkUpdate, fetchVersionHistory, type UpdateInfo, type VersionHistory } from '../utils/update';

// 当前应用版本（tauri.conf.json 的 version，来自核心 app 插件）。
// 启动即拉取一次；浏览器调试环境无 Tauri API，保持空串（界面隐藏该行）。
export const appVersion = ref('');
void getVersion()
  .then((v) => (appVersion.value = v ?? ''))
  .catch(() => {});

// 原地更新模式：插件检测结果（桌面环境）
export const inplaceUpdate = shallowRef<Update | null>(null);
// 回退模式：GitHub API 检测结果（浏览器调试环境）
export const apiInfo = ref<UpdateInfo | null>(null);
// 中央详细卡片开关
export const detailOpen = ref(false);
// 检测中标记（设置菜单"检查更新"按钮用）
export const checking = ref(false);
// 最近一次检测的时间文字（设置按钮下方提示用）
export const lastCheckText = ref('');
// 下载/安装进行中标记（UpdateButton 维护）：定时静默检查在此期间跳过，
// 避免检测替换 Update 实例、打断正在进行的下载/安装。
export const updateRunning = ref(false);

export const hasUpdate = computed(() => !!inplaceUpdate.value || !!apiInfo.value?.hasUpdate);
// 是否有更新链路操作正在进行（检测中 / 下载中 / 安装中）
export const updateBusy = computed(() => checking.value || updateRunning.value);
export const latestVersion = computed(
  () => inplaceUpdate.value?.version ?? apiInfo.value?.latestVersion ?? ''
);
export const currentVersion = computed(
  () => inplaceUpdate.value?.currentVersion ?? apiInfo.value?.currentVersion ?? ''
);
export const changelog = computed(() => inplaceUpdate.value?.body ?? apiInfo.value?.changelog ?? '');

// ---------- 分版本更新历史（懒加载） ----------
// 详细卡片打开时才请求：当前版本 → 最新版本之间的每个 Release 说明。
// 拉取成功按"当前版本号"缓存（会话内不重复请求）；失败置空，下次打开重试。
export const versionHistory = ref<VersionHistory[]>([]);
export const historyLoading = ref(false);
let historyLoadedFor = '';

const loadVersionHistory = async () => {
  const current = currentVersion.value;
  if (!current || historyLoading.value || (historyLoadedFor === current && versionHistory.value.length > 0)) return;
  historyLoading.value = true;
  try {
    const list = await fetchVersionHistory(current);
    versionHistory.value = list;
    if (list.length > 0) historyLoadedFor = current;
  } finally {
    historyLoading.value = false;
  }
};

// 打开详细卡片：同时懒加载分版本历史（失败静默，卡片内回退显示最新版说明）
export const openDetailCard = () => {
  detailOpen.value = true;
  if (hasUpdate.value) void loadVersionHistory();
};

// ---------- 定时静默检查 ----------
// 启动即查一次 + 每 4 小时复查：只更新内部状态（标题栏绿色「更新」徽标随之出现/消失），
// 不弹任何卡片或提示——详细内容用户主动点徽标/手动检查时才展示。
let periodicTimer: ReturnType<typeof setInterval> | undefined;

export const startPeriodicUpdateCheck = () => {
  void runUpdateCheck().catch(() => {});
  periodicTimer ??= setInterval(() => {
    // 正在检测或下载/安装中：跳过本轮，避免并发请求与实例被替换
    if (updateBusy.value) return;
    void runUpdateCheck().catch(() => {});
  }, 4 * 60 * 60 * 1000);
};

export const stopPeriodicUpdateCheck = () => {
  if (periodicTimer) {
    clearInterval(periodicTimer);
    periodicTimer = undefined;
  }
};

// 执行一次检测：桌面优先 updater 插件（原地更新），失败回退 GitHub API。
// 返回 'update' | 'latest' | 'error' 供调用方提示。
// checking 复位统一放外层 finally——检测到更新的路径会提前 return，
// 此前复位只在第二个 try 的 finally 里，导致"检查中"状态卡死。
export type UpdateCheckResult = 'update' | 'latest' | 'error';

// 共享进行中的检测 Promise：并发调用（启动定时检查 + 用户手动检查）复用同一次请求，
// 避免同时向更新源发多个请求、后到的结果覆盖先到的 Update 实例。
let checkInFlight: Promise<UpdateCheckResult> | null = null;

const doUpdateCheck = async (): Promise<UpdateCheckResult> => {
  checking.value = true;
  try {
    try {
      const update = (await check()) ?? null;
      // 替换实例前释放旧实例：Update 背后是 Rust 侧资源句柄，不能只丢引用。
      // 下载/安装进行中时不动旧实例——UpdateButton 已捕获局部引用，立即 close 会中断下载。
      const stale = inplaceUpdate.value;
      inplaceUpdate.value = update;
      if (stale && stale !== update && !updateRunning.value) void stale.close().catch(() => {});
      if (update) {
        lastCheckText.value = `已检测到新版本 v${update.version}`;
        return 'update';
      }
    } catch {
      /* 浏览器调试环境 / 插件不可用 → 走 GitHub API 回退 */
    }
    const info = await checkUpdate();
    apiInfo.value = info;
    if (!info) {
      // checkUpdate 返回 null = 网络/接口失败（与"已是最新"必须区分，不能误报最新）
      lastCheckText.value = '检测失败，请检查网络';
      return 'error';
    }
    if (info.hasUpdate) {
      lastCheckText.value = `已检测到新版本 v${info.latestVersion}`;
      return 'update';
    }
    lastCheckText.value = '当前已是最新版本';
    return 'latest';
  } catch {
    // 检测异常：清空可能残留的 Update 实例，避免徽标指向已被释放的资源
    const stale = inplaceUpdate.value;
    inplaceUpdate.value = null;
    if (stale) void stale.close().catch(() => {});
    lastCheckText.value = '检测失败，请检查网络';
    return 'error';
  } finally {
    checking.value = false;
  }
};

export const runUpdateCheck = (): Promise<UpdateCheckResult> => {
  // 重入复用：已有检测在途时直接返回同一个 Promise（不再发第二次请求）
  if (checkInFlight) return checkInFlight;
  checkInFlight = doUpdateCheck().finally(() => {
    checkInFlight = null;
  });
  return checkInFlight;
};
