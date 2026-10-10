<script setup lang="ts">
import { onMounted, ref, watch } from 'vue';
import { open } from '@tauri-apps/plugin-dialog';
import { installedList, isEnabled, setEnabled, syncInstalledFromDisk, uninstallPlugin } from '../plugins/host';
import { fetchMarketIndex, installFromMarket, importLocalPlugin, type MarketEntry } from '../plugins/market';
import { MARKET_SOURCES, readMarketSource, writeMarketSource } from '../plugins/config';
import { appliedTheme, notify } from '../stores/appStore';
import { compareVersion } from '../utils/update';

// 插件管理（设置入口的中央面板）：已安装列表（启停/卸载/本地导入）+ 插件市场（拉索引、下载安装）。
// Teleport 到 body 自持主题类，风格与更新卡片一致。

const emit = defineEmits<{ close: [] }>();

const errMsg = (e: unknown) => (e instanceof Error ? e.message : String(e));

// ---------- 已安装 ----------
const tab = ref<'installed' | 'market'>('installed');
const busy = ref(false);
// 头部刷新（与"新建连接会话"右侧同款）：全量对账磁盘（补上已装、清掉已卸、重试失败项）
const syncing = ref(false);
const onSync = async () => {
  if (syncing.value) return;
  syncing.value = true;
  try {
    await syncInstalledFromDisk();
  } finally {
    syncing.value = false;
  }
};
const confirmId = ref('');

const sourceLabel = (source: string) => (source === 'builtin' ? '内置' : source === 'market' ? '市场' : '本地');

// 单按钮开关：点击在 启用/停用 之间切换（原单选框样式废弃）
const togglePlugin = (id: string) => {
  const on = !isEnabled(id);
  setEnabled(id, on);
  notify(on ? `插件 ${id} 已启用` : `插件 ${id} 已停用（视图从功能切换移除，列表保留）`);
};

const onUninstall = async (id: string) => {
  if (confirmId.value !== id) {
    confirmId.value = id; // 二次确认：3 秒内再点一次才真正卸载
    setTimeout(() => {
      if (confirmId.value === id) confirmId.value = '';
    }, 3000);
    return;
  }
  confirmId.value = '';
  try {
    await uninstallPlugin(id);
    notify(`插件 ${id} 已卸载`);
  } catch (e) {
    notify(`卸载失败：${errMsg(e)}`);
  }
};

const onImport = async () => {
  try {
    const path = await open({
      multiple: false,
      filters: [{ name: '插件包', extensions: ['zip'] }],
      title: '导入插件包（zip）',
    });
    if (typeof path !== 'string' || !path) return;
    busy.value = true;
    await importLocalPlugin(path);
    notify('插件导入成功，已加入功能切换列表');
  } catch (e) {
    notify(`导入失败：${errMsg(e)}`);
  } finally {
    busy.value = false;
  }
};

// ---------- 插件市场 ----------
const marketEntries = ref<MarketEntry[]>([]);
const marketLoading = ref(false);
const marketError = ref('');
const installingId = ref('');
const sourceUrl = ref(readMarketSource());

const loadMarket = async () => {
  marketLoading.value = true;
  marketError.value = '';
  try {
    const index = await fetchMarketIndex();
    marketEntries.value = index.plugins;
    if (index.plugins.length === 0) marketError.value = '市场暂无上架插件';
  } catch (e) {
    marketError.value = errMsg(e);
    marketEntries.value = [];
  } finally {
    marketLoading.value = false;
  }
};

const saveSource = () => {
  writeMarketSource(sourceUrl.value);
  notify('插件市场源已保存');
  void loadMarket();
};

const resetSource = () => {
  sourceUrl.value = MARKET_SOURCES[0];
  writeMarketSource(sourceUrl.value);
  void loadMarket();
};

const isInstalled = (id: string) => installedList.value.some((p) => p.id === id);

const hasUpdate = (entry: MarketEntry) => {
  const installed = installedList.value.find((p) => p.id === entry.id);
  return !!installed && compareVersion(entry.version, installed.version) > 0;
};

const onInstall = async (entry: MarketEntry) => {
  installingId.value = entry.id;
  try {
    await installFromMarket(entry);
    notify(`已安装 ${entry.name} v${entry.version}，已加入功能切换列表`);
  } catch (e) {
    notify(`安装失败：${errMsg(e)}`);
  } finally {
    installingId.value = '';
  }
};

watch(tab, (t) => {
  if (t === 'market' && !marketLoading.value && marketEntries.value.length === 0 && !marketError.value) {
    void loadMarket();
  }
});

// 面板每次打开都对账磁盘：已安装列表自愈（补上已装、清掉已卸、重试失败项）
onMounted(() => {
  void onSync();
});
</script>

<template>
  <Teleport to="body">
    <div class="pm-mask" @click="emit('close')">
      <div class="pm-card" :class="{ 'theme-dark': appliedTheme === 'dark' }" @click.stop>
        <button class="pm-close" title="关闭" @click="emit('close')">
          <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
            <line x1="18" y1="6" x2="6" y2="18"></line>
            <line x1="6" y1="6" x2="18" y2="18"></line>
          </svg>
        </button>
        <div class="pm-head">
          <h3 class="pm-title">插件管理</h3>
          <button
            class="small-btn"
            :disabled="syncing"
            title="重新扫描已安装插件：补上已装、清掉已卸、重试失败项"
            @click="onSync"
          >
            <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="23 4 23 10 17 10"></polyline>
              <polyline points="1 20 1 14 7 14"></polyline>
              <path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"></path>
            </svg>
          </button>
        </div>

        <div class="pm-tabs">
          <button :class="{ on: tab === 'installed' }" @click="tab = 'installed'">已安装（{{ installedList.length }}）</button>
          <button :class="{ on: tab === 'market' }" @click="tab = 'market'">插件市场</button>
        </div>

        <!-- 已安装 -->
        <div v-if="tab === 'installed'" class="pm-list">
          <div v-for="p in installedList" :key="p.id" class="pm-item">
            <div class="pm-item-head">
              <span class="pm-name">{{ p.name }}</span>
              <span class="pm-ver">v{{ p.version }}</span>
              <span class="pm-badge" :class="p.type === 'view' ? 'src-local' : 'src-market'">
                {{ p.type === 'view' ? '视图' : '协议' }}
              </span>
              <span class="pm-badge" :class="`src-${p.source}`">{{ sourceLabel(p.source) }}</span>
            </div>
            <p v-if="p.description" class="pm-desc">{{ p.description }}</p>
            <p v-if="p.author || p.id" class="pm-meta">
              {{ [p.author ? `作者 ${p.author}` : '', p.id ? `id ${p.id}` : ''].filter(Boolean).join(' · ') }}
            </p>
            <p v-if="p.error" class="pm-err">{{ p.error }}</p>
            <div class="pm-actions">
              <button
                v-if="p.source !== 'builtin'"
                class="pm-toggle"
                :class="{ off: !p.enabled }"
                :title="p.enabled ? '点击停用（视图从功能切换移除，列表保留）' : '点击启用（视图回到功能切换尾部）'"
                @click="togglePlugin(p.id)"
              >
                {{ p.enabled ? '已启用' : '已停用' }}
              </button>
              <span v-else class="pm-builtin-hint">内置插件，随应用提供</span>
              <button
                v-if="p.source !== 'builtin'"
                class="pm-btn danger"
                :class="{ confirm: confirmId === p.id }"
                @click="onUninstall(p.id)"
              >
                {{ confirmId === p.id ? '确认卸载？' : '卸载' }}
              </button>
            </div>
          </div>
          <button class="pm-btn import" :disabled="busy" @click="onImport">
            {{ busy ? '导入中…' : '导入本地插件包（.zip）' }}
          </button>
        </div>

        <!-- 插件市场 -->
        <div v-else class="pm-list">
          <div class="pm-source-row">
            <input v-model="sourceUrl" class="pm-source-input" spellcheck="false" placeholder="index.json 地址" />
            <button class="pm-btn" title="保存市场源" @click="saveSource">保存</button>
            <button class="pm-btn" title="恢复默认源" @click="resetSource">默认</button>
            <button class="pm-btn" :disabled="marketLoading" @click="loadMarket">{{ marketLoading ? '刷新中…' : '刷新' }}</button>
          </div>

          <p v-if="marketLoading" class="pm-hint">正在获取插件列表…</p>
          <p v-else-if="marketError" class="pm-err">{{ marketError }}</p>

          <div v-for="entry in marketEntries" :key="entry.id" class="pm-item">
            <div class="pm-item-head">
              <span class="pm-name">{{ entry.name }}</span>
              <span class="pm-ver">v{{ entry.version }}</span>
              <span class="pm-badge" :class="entry.type === 'view' ? 'src-local' : 'src-market'">
                {{ entry.type === 'view' ? '视图' : '协议' }}
              </span>
            </div>
            <p v-if="entry.description" class="pm-desc">{{ entry.description }}</p>
            <p class="pm-meta">
              {{ entry.author ? `作者 ${entry.author} · ` : '' }}{{ entry.size ? `${Math.ceil(entry.size / 1024)} KB` : '' }}
            </p>
            <div class="pm-actions">
              <button
                v-if="!isInstalled(entry.id) || hasUpdate(entry)"
                class="pm-btn primary"
                :disabled="installingId === entry.id"
                @click="onInstall(entry)"
              >
                {{ installingId === entry.id ? '安装中…' : hasUpdate(entry) ? `更新到 v${entry.version}` : '安装' }}
              </button>
              <span v-else class="pm-ok">已安装</span>
            </div>
          </div>
        </div>

        <p class="pm-note">
          第三方插件仅获得其声明的有限能力（收发数据/本地存储），无法接触系统接口；安装前请确认来源可信。
        </p>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.pm-mask {
  position: fixed;
  inset: 0;
  z-index: 200;
  display: grid;
  place-items: center;
  background: rgba(0, 0, 0, 0.45);
}

.pm-card {
  position: relative;
  width: 560px;
  max-width: calc(100vw - 48px);
  max-height: calc(100vh - 96px);
  overflow: auto;
  padding: 18px 20px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  background: #ffffff;
  border: 1px solid rgba(23, 26, 33, 0.12);
  border-radius: 10px;
  box-shadow: 0 24px 64px rgba(15, 23, 42, 0.3);
}

.pm-close {
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

.pm-close:hover {
  background: rgba(199, 69, 65, 0.1);
  color: #c74541;
}

.pm-title {
  margin: 0;
  font-size: 16px;
  color: #23262b;
}

.pm-head {
  display: flex;
  align-items: center;
  gap: 8px;
}

.pm-tabs {
  display: flex;
  gap: 2px;
  padding: 3px;
  background: rgba(23, 26, 33, 0.06);
  border-radius: 8px;
}

.pm-tabs button {
  flex: 1;
  padding: 6px 0;
  background: transparent;
  color: #4b5563;
  font-size: 13px;
}

.pm-tabs button.on {
  background: #ffffff;
  color: #23262b;
  box-shadow: 0 1px 4px rgba(15, 23, 42, 0.14);
}

.pm-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-height: 120px;
}

.pm-item {
  padding: 10px 12px;
  border: 1px solid rgba(23, 26, 33, 0.1);
  border-radius: 8px;
  background: rgba(248, 249, 251, 0.8);
}

.pm-item-head {
  display: flex;
  align-items: center;
  gap: 8px;
}

.pm-name {
  font-size: 13px;
  font-weight: 700;
  color: #23262b;
}

.pm-ver {
  font-size: 11px;
  color: #8a9099;
  font-family: Consolas, monospace;
}

.pm-badge {
  padding: 1px 8px;
  border-radius: 999px;
  font-size: 10px;
  font-weight: 600;
}

.src-builtin {
  color: #6b7280;
  background: rgba(107, 114, 128, 0.14);
}

.src-local {
  color: #2b6cb0;
  background: rgba(59, 111, 212, 0.12);
}

.src-market {
  color: #2e8b45;
  background: rgba(46, 139, 69, 0.12);
}

.pm-desc {
  margin: 6px 0 0;
  font-size: 12px;
  line-height: 1.6;
  color: #4b5563;
}

.pm-meta {
  margin: 4px 0 0;
  font-size: 11px;
  color: #9ca3af;
}

.pm-err {
  margin: 6px 0 0;
  font-size: 12px;
  color: #c74541;
  word-break: break-all;
}

.pm-actions {
  margin-top: 8px;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
}

.pm-toggle {
  padding: 4px 12px;
  font-size: 12px;
  border-radius: 999px;
  background: rgba(46, 184, 92, 0.12);
  color: #2e8b45;
  box-shadow: inset 0 0 0 1px rgba(46, 184, 92, 0.35);
  cursor: pointer;
  margin-right: auto;
  white-space: nowrap;
}

.pm-toggle:hover {
  background: rgba(46, 184, 92, 0.2);
}

.pm-toggle.off {
  background: rgba(23, 26, 33, 0.05);
  color: #8a9099;
  box-shadow: inset 0 0 0 1px rgba(23, 26, 33, 0.12);
}

.pm-toggle.off:hover {
  background: rgba(23, 26, 33, 0.1);
  color: #4b5563;
}

.pm-builtin-hint {
  font-size: 11px;
  color: #9ca3af;
  margin-right: auto;
}

.pm-btn {
  padding: 5px 12px;
  font-size: 12px;
  background: rgba(23, 26, 33, 0.05);
  color: #4b5563;
  box-shadow: inset 0 0 0 1px rgba(23, 26, 33, 0.1);
  white-space: nowrap;
}

.pm-btn:hover:not(:disabled) {
  background: rgba(59, 111, 212, 0.08);
  color: #3563c2;
}

.pm-btn.primary {
  background: rgba(46, 184, 92, 0.12);
  color: #2e8b45;
  box-shadow: inset 0 0 0 1px rgba(46, 184, 92, 0.35);
}

.pm-btn.danger:hover:not(:disabled) {
  background: rgba(199, 69, 65, 0.1);
  color: #c74541;
}

.pm-btn.danger.confirm {
  background: rgba(199, 69, 65, 0.12);
  color: #c74541;
  box-shadow: inset 0 0 0 1px rgba(199, 69, 65, 0.4);
}

.pm-btn.import {
  width: 100%;
  padding: 8px 0;
}

.pm-ok {
  font-size: 12px;
  color: #2e8b45;
}

.pm-source-row {
  display: flex;
  gap: 6px;
}

.pm-source-input {
  flex: 1;
  min-width: 0;
  font-size: 12px;
  padding: 6px 10px;
}

.pm-hint {
  margin: 0;
  font-size: 12px;
  color: #8a9099;
  text-align: center;
  padding: 12px 0;
}

.pm-note {
  margin: 0;
  font-size: 11px;
  line-height: 1.6;
  color: #8a9099;
}

/* 深色主题 */
.pm-card.theme-dark {
  background: #2b2d30;
  border-color: rgba(255, 255, 255, 0.1);
}

.pm-card.theme-dark .pm-title {
  color: #f4f4f6;
}

.pm-card.theme-dark .pm-tabs {
  background: rgba(255, 255, 255, 0.06);
}

.pm-card.theme-dark .pm-tabs button {
  color: #9da0a8;
}

.pm-card.theme-dark .pm-tabs button.on {
  background: rgba(255, 255, 255, 0.16);
  color: #f4f4f6;
  box-shadow: none;
}

.pm-card.theme-dark .pm-item {
  background: #1e1f22;
  border-color: rgba(255, 255, 255, 0.09);
}

.pm-card.theme-dark .pm-name {
  color: #f4f4f6;
}

.pm-card.theme-dark .pm-desc {
  color: #b3b7be;
}

.pm-card.theme-dark .pm-btn {
  background: rgba(255, 255, 255, 0.06);
  color: #b3b7be;
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.1);
}

.pm-card.theme-dark .pm-btn.primary {
  background: rgba(107, 201, 126, 0.16);
  color: #6bc97e;
  box-shadow: inset 0 0 0 1px rgba(107, 201, 126, 0.4);
}

.pm-card.theme-dark .pm-source-input {
  background: #1e1f22;
  border-color: rgba(255, 255, 255, 0.1);
  color: #d6d9de;
}

.pm-card.theme-dark .pm-toggle {
  background: rgba(107, 201, 126, 0.16);
  color: #6bc97e;
  box-shadow: inset 0 0 0 1px rgba(107, 201, 126, 0.4);
}

.pm-card.theme-dark .pm-toggle:hover {
  background: rgba(107, 201, 126, 0.26);
}

.pm-card.theme-dark .pm-toggle.off {
  background: rgba(255, 255, 255, 0.06);
  color: #9da0a8;
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.1);
}

.pm-card.theme-dark .pm-note {
  color: #7c828c;
}
</style>
