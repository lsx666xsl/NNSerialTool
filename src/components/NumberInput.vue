<script setup lang="ts">
// 数字输入组件：替代原生 type=number（原生上下箭头仅悬停可见且样式无法定制）。
// 右侧常显上/下步进按钮，配色与 SelfSelect 下拉箭头一致，深浅主题自适应。
const props = withDefaults(
  defineProps<{
    modelValue: number;
    min?: number;
    max?: number;
    step?: number;
    placeholder?: string;
    disabled?: boolean;
  }>(),
  { min: 0, max: 999999999, step: 1, placeholder: '', disabled: false }
);

const emit = defineEmits<{ (e: 'update:modelValue', v: number): void }>();

const clamp = (v: number) => Math.min(props.max, Math.max(props.min, v));

// 输入过程中即时上抛合法数字；清空/非法时保持原值不动（失焦恢复显示）
const onInput = (e: Event) => {
  const n = parseInt((e.target as HTMLInputElement).value, 10);
  if (!Number.isNaN(n)) emit('update:modelValue', clamp(n));
};

// 失焦时把显示值规范化回当前模型值（清空后自动恢复）
const onBlur = (e: Event) => {
  (e.target as HTMLInputElement).value = String(props.modelValue);
};

const bump = (dir: 1 | -1) => {
  if (props.disabled) return;
  const base = Number.isFinite(props.modelValue) ? props.modelValue : props.min;
  emit('update:modelValue', clamp(base + dir * props.step));
};
</script>

<template>
  <div class="num-field" :class="{ disabled }">
    <input
      :value="modelValue"
      type="text"
      inputmode="numeric"
      autocomplete="off"
      :placeholder="placeholder"
      :disabled="disabled"
      @input="onInput"
      @blur="onBlur"
    />
    <div class="num-btns">
      <button class="num-btn" tabindex="-1" title="增加" :disabled="disabled" @click="bump(1)">
        <svg viewBox="0 0 24 24" width="10" height="10" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round">
          <polyline points="6 15 12 9 18 15"></polyline>
        </svg>
      </button>
      <button class="num-btn" tabindex="-1" title="减少" :disabled="disabled" @click="bump(-1)">
        <svg viewBox="0 0 24 24" width="10" height="10" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round">
          <polyline points="6 9 12 15 18 9"></polyline>
        </svg>
      </button>
    </div>
  </div>
</template>

<style scoped>
.num-field {
  position: relative;
  display: inline-flex;
  width: 100%;
}

.num-field.disabled {
  opacity: 0.55;
}

.num-field input {
  width: 100%;
  padding-right: 28px;
}

/* 步进按钮区：与下拉箭头同款观感——透明底（跟随输入框底色）、仅图标、hover 变色。
   水平对齐：图标中心距输入框右缘 16px，与 SelfSelect 箭头一致 */
.num-btns {
  position: absolute;
  /* 7px = 下拉箭头的 1px 边框 + 10px 内边距 - 按钮半宽 10px + 6px 箭头半径 → 中心距右缘 17px 与下拉箭头完全对齐 */
  right: 7px;
  top: 2px;
  bottom: 2px;
  display: flex;
  flex-direction: column;
  justify-content: center;
}

.num-btn {
  flex: 1;
  width: 20px;
  max-height: 16px;
  padding: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 3px;
  background: transparent;
  color: #6b7280;
  box-shadow: none;
}

.num-btn:hover:not(:disabled) {
  background: transparent;
  color: #23262b;
}

/* 深色主题：底色随输入框（近黑），仅图标色提亮 */
.theme-dark .num-btn {
  background: transparent;
  color: #9da0a8;
}

.theme-dark .num-btn:hover:not(:disabled) {
  background: transparent;
  color: #f4f4f6;
}
</style>
