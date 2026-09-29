<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import NumberInput from './NumberInput.vue';
import SelfSelect from './SelfSelect.vue';
import {
  addCustomBaudRate,
  appliedTheme,
  canCreateSession,
  createSession,
  dataBits,
  baudRates,
  localIps,
  newNetConfig,
  newSessionConfig,
  newSessionType,
  parityBits,
  ports,
  refreshPorts,
  stopBits,
} from '../stores/appStore';
import type { ConnectionType } from '../types';

// 新建会话表单：连接类型切换后展示串口参数或网络参数两套字段。
// 全部下拉统一使用 SelfSelect 自绘组件（浮层贴合显示框、深浅主题自适配）。
// 网络地址为自绘输入下拉：点击箭头在输入框正下方展开浮层（本机 IP + 常用地址），也支持手敲任意地址。
const showCustomBaud = ref(false);
const customBaudRate = ref('');

// 连接类型选项（SelfSelect 使用）
const typeOptions: Array<{ value: ConnectionType; label: string }> = [
  { value: 'serial', label: '串口' },
  { value: 'tcp_client', label: 'TCP Client' },
  { value: 'tcp_server', label: 'TCP Server' },
  { value: 'udp', label: 'UDP' },
];

// 地址下拉的展开状态；点击浮层外部自动收起
const addrOpen = ref(false);
const addrWrap = ref<HTMLElement | null>(null);
const addrPopEl = ref<HTMLElement | null>(null);
// 浮层 Teleport 到 body 后用 fixed 定位（不占布局，展开不会顶动侧栏）
const addrPopStyle = ref<{ top: string; left: string; width: string }>({ top: '0px', left: '0px', width: '0px' });
// 展开时记录触发框位置：只收起"锚点真的移动了"的滚动（消息流自动滚动不收起）
let addrOpenRect: { left: number; top: number } | null = null;

const toggleAddr = () => {
  if (addrOpen.value) {
    addrOpen.value = false;
    return;
  }
  const input = addrWrap.value?.querySelector('input');
  if (!input) return;
  const rect = input.getBoundingClientRect();
  // 根节点 zoom 缩放下 fixed 定位坐标换算（同 SelfSelect）
  const zoom = Number(document.documentElement.style.zoom) || 1;
  addrPopStyle.value = {
    top: `${rect.bottom / zoom}px`,
    left: `${rect.left / zoom}px`,
    width: `${rect.width / zoom}px`,
  };
  addrOpenRect = { left: rect.left, top: rect.top };
  addrOpen.value = true;
};

const onDocClick = (e: MouseEvent) => {
  const t = e.target as HTMLElement;
  if (addrWrap.value?.contains(t) || t.closest('.addr-pop')) return;
  addrOpen.value = false;
};

const onWinScrollOrResize = (e: Event) => {
  if (!addrOpen.value) return;
  if (e.target instanceof Node && addrPopEl.value?.contains(e.target)) return;
  const box = addrWrap.value;
  if (box) {
    const rect = box.getBoundingClientRect();
    if (addrOpenRect && Math.abs(rect.left - addrOpenRect.left) < 1 && Math.abs(rect.top - addrOpenRect.top) < 1) return;
  }
  addrOpen.value = false;
};

onMounted(() => {
  document.addEventListener('click', onDocClick);
  window.addEventListener('scroll', onWinScrollOrResize, true);
  window.addEventListener('resize', onWinScrollOrResize);
});
onUnmounted(() => {
  document.removeEventListener('click', onDocClick);
  window.removeEventListener('scroll', onWinScrollOrResize, true);
  window.removeEventListener('resize', onWinScrollOrResize);
});

// 地址建议列表：常用回环/通配地址 + 扫描到的本机网卡 IP
const addressSuggestions = computed(() => {
  const base = ['127.0.0.1'];
  if (newSessionType.value === 'tcp_server') base.unshift('0.0.0.0');
  return [...base, ...localIps.value.filter((ip) => ip !== '127.0.0.1' && ip !== '0.0.0.0')];
});

const pickAddress = (ip: string) => {
  newNetConfig.value.host = ip;
  addrOpen.value = false;
};

// 自定义波特率：回车或失焦时提交，非法输入静默还原
const onCustomBaudBlur = () => {
  if (addCustomBaudRate(customBaudRate.value)) {
    customBaudRate.value = '';
    showCustomBaud.value = false;
  }
};
</script>

<template>
  <section class="panel">
    <div class="section-title">
      <h2>新建连接会话</h2>
      <button class="ghost-btn" title="刷新串口与网卡列表" @click="refreshPorts">
        <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
          <polyline points="23 4 23 10 17 10"></polyline>
          <polyline points="1 20 1 14 7 14"></polyline>
          <path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"></path>
        </svg>
      </button>
    </div>

    <div class="form-row">
      <label>连接类型</label>
      <SelfSelect v-model="newSessionType" :options="typeOptions" />
    </div>

    <template v-if="newSessionType === 'serial'">
      <div class="form-row">
        <label>端口号</label>
        <SelfSelect
          v-model="newSessionConfig.port"
          :options="ports.map((p) => ({ value: p.portnum, label: `${p.portnum} - ${p.portproduct}` }))"
          placeholder="请选择串口"
        />
      </div>

      <div class="form-row">
        <label>波特率</label>
        <SelfSelect v-model="newSessionConfig.baudRate" :options="baudRates.map((b) => ({ value: b, label: String(b) }))" />
        <button v-if="!showCustomBaud" class="small-btn" title="自定义波特率" @click="showCustomBaud = true">+</button>
        <input
          v-else
          v-model="customBaudRate"
          placeholder="自定义"
          @keyup.enter="onCustomBaudBlur"
          @blur="onCustomBaudBlur"
        />
      </div>

      <div class="form-row compact-row">
        <label>数据位</label>
        <SelfSelect v-model="newSessionConfig.dataBits" :options="dataBits.map((d) => ({ value: d, label: String(d) }))" />
      </div>

      <div class="form-row compact-row">
        <label>校验位</label>
        <SelfSelect v-model="newSessionConfig.parityBits" :options="parityBits.map((p) => ({ value: p, label: p }))" />
      </div>

      <div class="form-row compact-row">
        <label>停止位</label>
        <SelfSelect v-model="newSessionConfig.stopBits" :options="stopBits.map((s) => ({ value: s, label: String(s) }))" />
      </div>
    </template>

    <template v-else>
      <div class="form-row">
        <label>{{ newSessionType === 'tcp_server' ? '监听地址' : '目标地址' }}</label>
        <!-- 自绘地址下拉：输入框 + 箭头一体，浮层固定显示在输入框正下方，样式与其他下拉控件统一 -->
        <div ref="addrWrap" class="addr-field">
          <input v-model="newNetConfig.host" :placeholder="newSessionType === 'tcp_server' ? '0.0.0.0' : '127.0.0.1'" />
          <button class="addr-arrow" tabindex="-1" @click="toggleAddr">
            <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="6 9 12 15 18 9"></polyline>
            </svg>
          </button>
        </div>
      </div>
      <div class="form-row">
        <label>{{ newSessionType === 'tcp_server' ? '监听端口' : '目标端口' }}</label>
        <NumberInput v-model="newNetConfig.port" :min="1" :max="65535" />
      </div>
      <div v-if="newSessionType === 'udp'" class="form-row">
        <label>本地地址</label>
        <input v-model="newNetConfig.localHost" placeholder="0.0.0.0" />
      </div>
      <div v-if="newSessionType === 'udp'" class="form-row">
        <label>本地端口</label>
        <NumberInput v-model="newNetConfig.localPort" :min="0" :max="65535" />
      </div>
    </template>

    <button class="primary-btn full-btn" :disabled="!canCreateSession" @click="createSession">添加会话</button>
  </section>

  <!-- 浮层 Teleport 到 body：fixed 定位跟随输入框，不占侧栏布局、不引起滚动 -->
  <Teleport to="body">
    <div v-if="addrOpen" ref="addrPopEl" class="addr-pop" :class="{ 'theme-dark': appliedTheme === 'dark' }" :style="addrPopStyle">
      <div
        v-for="ip in addressSuggestions"
        :key="ip"
        class="addr-opt"
        :class="{ active: ip === newNetConfig.host }"
        @click="pickAddress(ip)"
      >
        {{ ip }}
      </div>
      <div v-if="addressSuggestions.length === 0" class="addr-empty">暂无扫描结果</div>
    </div>
  </Teleport>
</template>

<style scoped>
/* 自绘地址下拉：输入框与箭头一体，浮层固定在输入框正下方 */
.addr-field {
  position: relative;
  width: 100%;
}

.addr-field input {
  width: 100%;
  padding-right: 30px;
}

.addr-arrow {
  position: absolute;
  right: 6px;
  top: 50%;
  transform: translateY(-50%);
  width: 22px;
  height: 22px;
  padding: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 4px;
  background: transparent;
  color: #6b7280;
  box-shadow: none;
}

/* 内嵌在输入框里的箭头不做悬停位移（覆盖全局 button:hover 的 translateY，避免箭头自己跳动） */
.addr-arrow:hover:not(:disabled) {
  transform: translateY(-50%);
  background: transparent;
}

.addr-arrow:hover {
  /* 与原生 select 箭头行为一致：仅变色，无背景浮起效果 */
  background: transparent;
  color: #23262b;
}

.addr-pop {
  position: fixed;
  /* 上边框完全贴合输入框下边框（间距 0） */
  top: 100%;
  left: 0;
  right: 0;
  z-index: 70;
  padding: 4px;
  background: #ffffff;
  border: 1px solid rgba(23, 26, 33, 0.12);
  border-radius: 8px;
  box-shadow: 0 12px 32px rgba(15, 23, 42, 0.16);
  max-height: 180px;
  overflow: auto;
}

.addr-opt {
  padding: 7px 10px;
  border-radius: 5px;
  font-size: 13px;
  color: #23262b;
  cursor: pointer;
}

.addr-opt:hover {
  background: rgba(59, 111, 212, 0.08);
}

.addr-opt.active {
  background: rgba(59, 111, 212, 0.12);
  color: #3563c2;
}

.addr-empty {
  padding: 8px 10px;
  font-size: 12px;
  color: #8a9099;
  text-align: center;
}

/* 深色主题 */
.theme-dark .addr-arrow {
  color: #9da0a8;
}

.theme-dark .addr-arrow:hover {
  background: transparent;
  color: #f4f4f6;
}

.addr-pop.theme-dark {
  background: #33353a;
  border-color: rgba(255, 255, 255, 0.12);
  box-shadow: 0 12px 36px rgba(0, 0, 0, 0.45);
}

.addr-pop.theme-dark .addr-opt {
  color: #dfdfe3;
}

.addr-pop.theme-dark .addr-opt:hover {
  background: rgba(87, 157, 245, 0.14);
}

.addr-pop.theme-dark .addr-opt.active {
  background: rgba(87, 157, 245, 0.2);
  color: #8fbdf7;
}
</style>
