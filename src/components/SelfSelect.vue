<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { appliedTheme } from '../stores/appStore';
import { fixedPxUnit } from '../utils/zoom';

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
// 定位信息：宽度用 min-width（浮层本体 max-content 撑开），保证短触发框也能完整显示长选项；
// maxWidth 会按最近的 panel/content-card 边界再收紧，避免长选项把浮层撑出新建会话面板。
const popStyle = ref<{
  left?: string;
  right?: string;
  minWidth: string;
  maxWidth: string;
  maxHeight: string;
  top?: string;
  bottom?: string;
}>({
  minWidth: '0px',
  maxWidth: '320px',
  maxHeight: '240px',
});
// 展开时记录触发框位置：用于区分"会移动锚点的滚动"与"无关滚动"（如消息流自动滚动）
let openRect: { left: number; top: number } | null = null;

const currentLabel = computed(() => {
  const hit = props.options.find((o) => o.value === props.modelValue);
  return hit ? hit.label : String(props.modelValue ?? '');
});

// ---------- 键盘可达性（无障碍） ----------
// 根节点作为 composite button 可 Tab 聚焦：Enter/Space 开关，Escape 关闭，
// 上下方向键移动高亮，展开时 Enter/Space 选中高亮项。
const hlIndex = ref(-1);
const resetHl = () => {
  const idx = props.options.findIndex((o) => o.value === props.modelValue);
  hlIndex.value = idx >= 0 ? idx : 0;
};
const moveHl = (step: number) => {
  const n = props.options.length;
  if (n === 0) return;
  hlIndex.value = (hlIndex.value + step + n) % n;
};
const onKeydown = (e: KeyboardEvent) => {
  if (props.disabled) return;
  switch (e.key) {
    case 'Enter':
    case ' ':
      e.preventDefault();
      if (!open.value) toggle();
      else {
        const opt = props.options[hlIndex.value];
        if (opt) pick(opt.value);
      }
      break;
    case 'Escape':
      if (open.value) {
        e.preventDefault();
        open.value = false;
      }
      break;
    case 'ArrowDown':
      e.preventDefault();
      if (!open.value) toggle();
      else moveHl(1);
      break;
    case 'ArrowUp':
      e.preventDefault();
      if (!open.value) toggle();
      else moveHl(-1);
      break;
    default:
      break;
  }
};

const toggle = () => {
  if (props.disabled) return;
  if (open.value) {
    open.value = false;
    return;
  }
  const box = wrapEl.value;
  if (!box) return;
  const rect = box.getBoundingClientRect();
  // fixed 浮层坐标换算：Chromium（WebView2）fixed 继承根节点 zoom，视觉像素需除以 zoom；
  // WebKitGTK 不继承，除反而错位。具体行为运行时探测，见 utils/zoom.ts。
  const unit = fixedPxUnit();
  // 视口边界钳制：下方放不下时收缩浮层高度出滚动条；再不够则整体向上展开
  const availBelow = (window.innerHeight - rect.bottom - 10) / unit;
  const availAbove = (rect.top - 10) / unit;
  const style: {
    left?: string;
    right?: string;
    minWidth: string;
    maxWidth: string;
    maxHeight: string;
    top?: string;
    bottom?: string;
  } = {
    minWidth: `${rect.width / unit}px`,
    maxWidth: '320px',
    maxHeight: '240px',
  };
  // 横向：默认与触发框左对齐；触发框太靠右时改锚右缘，防止 max-content 撑出右边界
  if (rect.right + 160 > window.innerWidth) {
    style.right = `${(window.innerWidth - rect.right) / unit}px`;
  } else {
    style.left = `${rect.left / unit}px`;
  }
  // 表单里的下拉不能比所在面板更宽：长串口产品名会把 max-content 浮层撑出面板右缘。
  // 按“锚点 → 最近面板边界”的可用空间收紧 maxWidth，超长选项在浮层内省略显示。
  const host = box.closest<HTMLElement>('.panel, .content-card');
  let maxWidth = 320;
  if (host) {
    const hostRect = host.getBoundingClientRect();
    const space = style.right !== undefined
      ? (rect.right - hostRect.left) / unit
      : (hostRect.right - rect.left) / unit;
    maxWidth = Math.max(64, Math.min(320, space - 6));
  }
  style.maxWidth = `${maxWidth}px`;
  // min-width 会覆盖 max-width；触发框本身比可用空间宽时也要一起收紧，
  // 否则 maxWidth 的 6px 安全边距会被 min-width 吃掉。
  style.minWidth = `${Math.min(rect.width / unit, maxWidth)}px`;
  if (availBelow >= 96 || availBelow >= availAbove) {
    style.top = `${rect.bottom / unit}px`;
    style.maxHeight = `${Math.max(96, Math.min(240, availBelow))}px`;
  } else {
    style.bottom = `${(window.innerHeight - rect.top) / unit}px`;
    style.maxHeight = `${Math.max(96, Math.min(240, availAbove))}px`;
  }
  popStyle.value = style;
  openRect = { left: rect.left, top: rect.top };
  resetHl();
  open.value = true;
};

const pick = (v: string | number) => {
  emit('update:modelValue', v);
  open.value = false;
  // 选择后把焦点交还触发框：键盘用户可继续操作，避免焦点随 Teleport 浮层一起消失
  wrapEl.value?.focus();
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
    tabindex="0"
    role="button"
    aria-haspopup="listbox"
    :aria-expanded="open"
    :aria-disabled="disabled"
    :aria-label="currentLabel || placeholder"
    @click="toggle"
    @keydown="onKeydown"
  >
    <span class="ss-label" :class="{ placeholder: currentLabel === placeholder && placeholder }">{{ currentLabel || placeholder }}</span>
    <svg class="ss-arrow" viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
      <polyline points="6 9 12 15 18 9"></polyline>
    </svg>
  </div>

  <Teleport to="body">
    <!-- @click.stop：浮层 Teleport 到 body，点击选项若继续冒泡会触发页面级的"点外部关闭"，
         导致设置菜单/侧栏浮层被误关（选项点击本身也会被同时处理） -->
    <div
      v-if="open"
      ref="popEl"
      class="self-select-pop"
      :class="{ 'theme-dark': appliedTheme === 'dark' }"
      :style="popStyle"
      role="listbox"
      @click.stop
    >
      <div
        v-for="(o, idx) in options"
        :key="o.value"
        class="ss-opt"
        :class="{ active: o.value === modelValue, hl: idx === hlIndex }"
        role="option"
        :aria-selected="o.value === modelValue"
        :title="o.label"
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
  min-width: 0;
}

.self-select.open,
.self-select:hover {
  border-color: rgba(56, 116, 203, 0.5);
}

.self-select.disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

/* 键盘聚焦提示：与输入框 focus 同观感（鼠标点击不显示，避免常驻描边） */
.self-select:focus-visible {
  outline: none;
  border-color: rgba(56, 116, 203, 0.65);
  box-shadow: 0 0 0 3px rgba(56, 116, 203, 0.14);
}

.ss-label {
  flex: 1 1 auto;
  min-width: 0;
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

/* 浮层：fixed 定位（Teleport 到 body），紧贴显示框下缘，无间距；
   宽度 max-content 撑开 + min-width 不小于触发框，长选项完整显示不截断 */
.self-select-pop {
  position: fixed;
  z-index: 90;
  width: max-content;
  max-width: 320px;
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
  /* 浮层被面板边界钳制后，超长选项在自身宽度内省略，避免横向滚动条 */
  overflow: hidden;
  text-overflow: ellipsis;
}

.ss-opt:hover {
  background: rgba(59, 111, 212, 0.08);
}

/* 键盘高亮项：与 hover 同观感，键盘移动时明确当前选中位置 */
.ss-opt.hl {
  background: rgba(59, 111, 212, 0.1);
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

.self-select-pop.theme-dark .ss-opt.hl {
  background: rgba(87, 157, 245, 0.16);
}

/* 深色主题下的键盘聚焦描边 */
.theme-dark .self-select:focus-visible {
  border-color: rgba(87, 157, 245, 0.6);
  box-shadow: 0 0 0 3px rgba(87, 157, 245, 0.16);
}

.self-select-pop.theme-dark .ss-opt.active {
  background: rgba(87, 157, 245, 0.2);
  color: #8fbdf7;
}
</style>
