<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { appliedTheme } from '../stores/appStore';

// 通用自绘下拉组件：替代原生 select（原生弹出层由系统渲染，位置/样式不可控且深色主题下泛白）。
// 结构：显示框（当前值 + 箭头）+ Teleport 到 body 的浮层（fixed 定位，紧贴显示框下缘）。
// 主题自持：浮层脱离组件祖先链，通过根节点自带 theme-dark 类适配深色。
const props = withDefaults(
  defineProps<{
    modelValue: string | number;
    options: Array<{ value: string | number; label: string }>;
    placeholder?: string;
    disabled?: boolean;
    // full：占满父容器宽度（form-row 的 1fr 列）；false 时自适应内容宽度
    full?: boolean;
  }>(),
  { placeholder: '请选择', disabled: false, full: true }
);

const emit = defineEmits<{ (e: 'update:modelValue', v: string | number): void }>();

const open = ref(false);
const wrapEl = ref<HTMLElement | null>(null);
const popEl = ref<HTMLElement | null>(null);
const popStyle = ref<{ top: string; left: string; width: string }>({ top: '0px', left: '0px', width: '0px' });
// 展开时记录触发框位置：用于区分"会移动锚点的滚动"与"无关滚动"（如消息流自动滚动）
let openRect: { left: number; top: number } | null = null;

const currentLabel = computed(() => {
  const hit = props.options.find((o) => o.value === props.modelValue);
  return hit ? hit.label : String(props.modelValue ?? '');
});

const toggle = () => {
  if (props.disabled) return;
  if (open.value) {
    open.value = false;
    return;
  }
  const box = wrapEl.value;
  if (!box) return;
  const rect = box.getBoundingClientRect();
  // 根节点带 zoom 整体缩放时，rect 是缩放后的视觉像素，而浮层的 fixed 定位
  // 解释的是布局像素——需除以 zoom 换算，否则浮层会偏向左上（缩放越小偏得越多）
  const zoom = Number(document.documentElement.style.zoom) || 1;
  popStyle.value = {
    top: `${rect.bottom / zoom}px`,
    left: `${rect.left / zoom}px`,
    width: `${rect.width / zoom}px`,
  };
  openRect = { left: rect.left, top: rect.top };
  open.value = true;
};

const pick = (v: string | number) => {
  emit('update:modelValue', v);
  open.value = false;
};

const onDocClick = (e: MouseEvent) => {
  const t = e.target as HTMLElement;
  if (wrapEl.value?.contains(t) || t.closest('.self-select-pop')) return;
  open.value = false;
};

// 滚动/缩放时收起浮层——但只收起"锚点真的移动了"的情况：
// 浮层选项列表自身的滚动、以及不移动触发框的滚动（接收数据时消息流自动滚动）
// 都不该把下拉关掉（否则接收期间下拉完全不可用、长列表永远滚不到底）
const onWinScrollOrResize = (e: Event) => {
  if (!open.value) return;
  // 浮层内部滚动（选项列表滚动条）：直接忽略
  if (e.target instanceof Node && popEl.value?.contains(e.target)) return;
  // 触发框位置未变（如消息流滚动、无关容器的滚动）：忽略
  const box = wrapEl.value;
  if (box) {
    const rect = box.getBoundingClientRect();
    if (openRect && Math.abs(rect.left - openRect.left) < 1 && Math.abs(rect.top - openRect.top) < 1) return;
  }
  open.value = false;
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
</script>

<template>
  <div
    ref="wrapEl"
    class="self-select"
    :class="{ full, disabled, 'theme-dark': appliedTheme === 'dark', open }"
    @click="toggle"
  >
    <span class="ss-label" :class="{ placeholder: currentLabel === placeholder && placeholder }">{{ currentLabel || placeholder }}</span>
    <svg class="ss-arrow" viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
      <polyline points="6 9 12 15 18 9"></polyline>
    </svg>
  </div>

  <Teleport to="body">
    <div v-if="open" ref="popEl" class="self-select-pop" :class="{ 'theme-dark': appliedTheme === 'dark' }" :style="popStyle">
      <div
        v-for="o in options"
        :key="o.value"
        class="ss-opt"
        :class="{ active: o.value === modelValue }"
        @click="pick(o.value)"
      >
        {{ o.label }}
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
/* 显示框：与输入框同观感（边框/圆角/高度/深浅主题） */
.self-select {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  border: 1px solid rgba(23, 26, 33, 0.16);
  border-radius: 6px;
  padding: 8px 10px;
  font-size: 13px;
  background-color: #ffffff;
  color: #23262b;
  cursor: pointer;
  user-select: none;
  transition: border-color 0.18s ease, box-shadow 0.18s ease;
}

.self-select.full {
  width: 100%;
}

.self-select.open,
.self-select:hover {
  border-color: rgba(56, 116, 203, 0.5);
}

.self-select.disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.ss-label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ss-label.placeholder {
  color: #9ca3af;
}

.ss-arrow {
  flex-shrink: 0;
  color: #6b7280;
  transition: transform 0.15s ease;
}

.self-select.open .ss-arrow {
  transform: rotate(180deg);
}

/* 浮层：fixed 定位（Teleport 到 body），紧贴显示框下缘，无间距 */
.self-select-pop {
  position: fixed;
  z-index: 90;
  padding: 4px;
  background: #ffffff;
  border: 1px solid rgba(23, 26, 33, 0.14);
  border-radius: 8px;
  box-shadow: 0 12px 32px rgba(15, 23, 42, 0.18);
  max-height: 240px;
  overflow: auto;
}

.ss-opt {
  padding: 7px 10px;
  border-radius: 5px;
  font-size: 13px;
  color: #23262b;
  cursor: pointer;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.ss-opt:hover {
  background: rgba(59, 111, 212, 0.08);
}

.ss-opt.active {
  background: rgba(59, 111, 212, 0.14);
  color: #3563c2;
}

/* 深色主题（浮层自持主题类 + 显示框依赖祖先类） */
.theme-dark .self-select {
  background-color: #1e1f22;
  border-color: rgba(255, 255, 255, 0.12);
  color: #dfdfe3;
}

.theme-dark .self-select.open,
.theme-dark .self-select:hover {
  border-color: rgba(87, 157, 245, 0.5);
}

.theme-dark .ss-arrow {
  color: #9da0a8;
}

.self-select-pop.theme-dark {
  background: #33353a;
  border-color: rgba(255, 255, 255, 0.12);
  box-shadow: 0 12px 36px rgba(0, 0, 0, 0.45);
}

.self-select-pop.theme-dark .ss-opt {
  color: #dfdfe3;
}

.self-select-pop.theme-dark .ss-opt:hover {
  background: rgba(87, 157, 245, 0.14);
}

.self-select-pop.theme-dark .ss-opt.active {
  background: rgba(87, 157, 245, 0.2);
  color: #8fbdf7;
}
</style>
