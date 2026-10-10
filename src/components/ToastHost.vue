<script setup lang="ts">
import { toasts } from '../stores/appStore';

// 中央通知宿主：INFO 类系统提示以浮层形式出现在界面中心，
// 出现后向上漂浮并逐渐消失（替代消息流内的 INFO 行与状态栏灰字）。
</script>

<template>
  <div class="toast-host">
    <div v-for="toast in toasts" :key="toast.id" class="toast">{{ toast.text }}</div>
  </div>
</template>

<style scoped>
.toast-host {
  position: fixed;
  top: 18%;
  left: 50%;
  transform: translateX(-50%);
  z-index: 300; /* 全局最顶层：中央通知必须压过一切弹窗（遮罩 200）与下拉 */
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  /* 通知纯展示，不挡底下界面的点击 */
  pointer-events: none;
}

.toast {
  max-width: 70vw;
  padding: 10px 18px;
  border-radius: 8px;
  background: rgba(38, 40, 45, 0.92);
  color: #f4f4f6;
  font-size: 13px;
  box-shadow: 0 10px 28px rgba(0, 0, 0, 0.35);
  white-space: pre-wrap;
  word-break: break-all;
  /* 生命周期动画：浮入 → 停留 → 上漂淡出（时长与 store 中的移除定时器一致） */
  animation: toast-life 2.6s ease forwards;
}

@keyframes toast-life {
  0% {
    opacity: 0;
    transform: translateY(14px);
  }
  10% {
    opacity: 1;
    transform: translateY(0);
  }
  72% {
    opacity: 1;
    transform: translateY(-6px);
  }
  100% {
    opacity: 0;
    transform: translateY(-30px);
  }
}
</style>
