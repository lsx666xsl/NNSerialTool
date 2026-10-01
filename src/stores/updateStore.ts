// 更新状态共享模块：标题栏更新徽标与设置里的"检查更新"按钮共用同一份检测状态，
// 保证两处入口看到的结果一致、中央详细卡片只有一个数据源。
// 注意：updater 插件返回的 Update 实例必须用 shallowRef（ref 的深度代理会让
// 插件类的私有字段访问抛 TypeError）。
import { computed, ref, shallowRef } from 'vue';
import { getVersion } from '@tauri-apps/api/app';
import { check, type Update } from '@tauri-apps/plugin-updater';
import { checkUpdate, type UpdateInfo } from '../utils/update';

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

export const hasUpdate = computed(() => !!inplaceUpdate.value || !!apiInfo.value?.hasUpdate);
export const latestVersion = computed(
  () => inplaceUpdate.value?.version ?? apiInfo.value?.latestVersion ?? ''
);
export const currentVersion = computed(
  () => inplaceUpdate.value?.currentVersion ?? apiInfo.value?.currentVersion ?? ''
);
export const changelog = computed(() => inplaceUpdate.value?.body ?? apiInfo.value?.changelog ?? '');

// 执行一次检测：桌面优先 updater 插件（原地更新），失败回退 GitHub API。
// 返回 'update' | 'latest' | 'error' 供调用方提示。
export const runUpdateCheck = async (): Promise<'update' | 'latest' | 'error'> => {
  checking.value = true;
  try {
    inplaceUpdate.value = (await check()) ?? null;
    if (inplaceUpdate.value) {
      lastCheckText.value = `已检测到新版本 v${inplaceUpdate.value.version}`;
      return 'update';
    }
  } catch {
    /* 浏览器调试环境 / 插件不可用 → 走 GitHub API 回退 */
  }
  try {
    apiInfo.value = await checkUpdate();
    if (apiInfo.value?.hasUpdate) {
      lastCheckText.value = `已检测到新版本 v${apiInfo.value.latestVersion}`;
      return 'update';
    }
    lastCheckText.value = '当前已是最新版本';
    return 'latest';
  } catch {
    lastCheckText.value = '检测失败，请检查网络';
    return 'error';
  } finally {
    checking.value = false;
  }
};
