<script setup lang="ts">
import { viewMode } from '../stores/appStore';
import type { ViewMode } from '../types';

// 视图切换：单屏 / 分屏 / 总览。
// 悬停时展示迷你布局缩略图 + 依次点亮的循环动画 + 一句话说明。
const viewModes: Array<{ key: ViewMode; label: string; tip: string; blocks: number }> = [
  { key: 'detail', label: '单屏', tip: '单连接大视图，完整参数与独立收发区', blocks: 3 },
  { key: 'split', label: '分屏', tip: '勾选的多个连接并排显示，互不干扰', blocks: 4 },
  { key: 'global', label: '总览', tip: '已加入总览的会话按时间线汇成一条流', blocks: 3 },
  { key: 'forward', label: '转发', tip: '配置会话之间的数据转发规则', blocks: 2 },
];
</script>

<template>
  <div class="view-switch">
    <div v-for="mode in viewModes" :key="mode.key" class="mode-btn-wrap">
      <button :class="{ selected: viewMode === mode.key }" @click="viewMode = mode.key">{{ mode.label }}</button>
      <div class="mode-tip">
        <div class="mini" :class="`mini-${mode.key}`">
          <i v-for="n in mode.blocks" :key="n" :style="{ animationDelay: `${(n - 1) * 0.4}s` }"></i>
        </div>
        <p>{{ mode.tip }}</p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.view-switch {
  display: flex;
  gap: 2px;
  background: rgba(23, 26, 33, 0.06);
  border-radius: 8px;
  padding: 3px;
}

.view-switch button {
  padding: 6px 14px;
  background: transparent;
  color: #4b5563;
}

.view-switch button.selected {
  background: #ffffff;
  color: #23262b;
  box-shadow: 0 1px 4px rgba(15, 23, 42, 0.14);
}

.mode-btn-wrap {
  position: relative;
}

.mode-tip {
  position: absolute;
  top: calc(100% + 10px);
  left: 50%;
  transform: translateX(-50%);
  z-index: 50;
  width: 176px;
  padding: 10px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  background: #ffffff;
  border: 1px solid rgba(23, 26, 33, 0.12);
  border-radius: 8px;
  box-shadow: 0 12px 32px rgba(15, 23, 42, 0.16);
  opacity: 0;
  visibility: hidden;
  transition: opacity 0.18s ease, visibility 0.18s ease;
  pointer-events: none;
}

.mode-btn-wrap:last-child .mode-tip {
  left: auto;
  right: 0;
  transform: none;
}

.mode-btn-wrap:hover .mode-tip {
  opacity: 1;
  visibility: visible;
}

.mode-tip p {
  font-size: 12px;
  color: #6b7280;
  text-align: center;
  margin: 0;
}

/* 迷你布局缩略图：几个方框 + 依次点亮的循环动画 */
.mini {
  display: grid;
  gap: 4px;
  padding: 6px;
  background: rgba(23, 26, 33, 0.05);
  border-radius: 6px;
}

.mini i {
  border-radius: 2px;
  background: rgba(59, 111, 212, 0.18);
  animation: miniBlink 2.4s infinite;
}

@keyframes miniBlink {
  0%,
  18% {
    background: rgba(59, 111, 212, 0.18);
  }
  28%,
  44% {
    background: rgba(59, 111, 212, 0.62);
  }
  54%,
  100% {
    background: rgba(59, 111, 212, 0.18);
  }
}

/* 详情：左侧窄栏 + 右侧上下两块 */
.mini-detail {
  grid-template-columns: 14px 1fr;
}

.mini-detail i {
  height: 16px;
}

.mini-detail i:first-child {
  grid-row: span 2;
}

/* 分屏：2x2 四块 */
.mini-split {
  grid-template-columns: 1fr 1fr;
}

.mini-split i {
  height: 16px;
}

/* 总线：三行横条依次流动 */
.mini-global {
  grid-template-columns: 1fr;
}

.mini-global i {
  height: 6px;
}

/* 转发：上下两块 + 连接 */
.mini-forward {
  grid-template-columns: 1fr;
}

.mini-forward i {
  height: 8px;
}

.mini-forward i:last-child {
  justify-self: end;
  width: 60%;
}

/* 深色主题 */
.theme-dark .view-switch {
  background: rgba(255, 255, 255, 0.06);
}

.theme-dark .view-switch button {
  color: #9da0a8;
}

.theme-dark .view-switch button.selected {
  background: rgba(255, 255, 255, 0.16);
  color: #f4f4f6;
  box-shadow: none;
}

.theme-dark .mode-tip {
  background: #33353a;
  border-color: rgba(255, 255, 255, 0.12);
  box-shadow: 0 12px 36px rgba(0, 0, 0, 0.45);
}

.theme-dark .mode-tip p {
  color: #9da0a8;
}

.theme-dark .mini {
  background: rgba(255, 255, 255, 0.05);
}

.theme-dark .mini i {
  background: rgba(87, 157, 245, 0.25);
  animation-name: miniBlinkDark;
}

@keyframes miniBlinkDark {
  0%,
  18% {
    background: rgba(87, 157, 245, 0.25);
  }
  28%,
  44% {
    background: rgba(87, 157, 245, 0.75);
  }
  54%,
  100% {
    background: rgba(87, 157, 245, 0.25);
  }
}
</style>
