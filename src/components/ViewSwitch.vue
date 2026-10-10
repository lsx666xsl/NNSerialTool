<script setup lang="ts">
import { computed } from 'vue';
import { viewMode } from '../stores/appStore';
import { views } from '../plugins/registry';

// 视图切换（数据驱动）：内置四视图 + 插件注册视图（自动排在"转发"之后）。
// 悬停时展示迷你布局缩略图 + 依次点亮的循环动画 + 一句话说明。
// 视图切换（数据驱动）：内置四视图在前（固定顺序），插件注册视图按注册序排在
// "转发"之后——排序确定化，不依赖注册时机（HMR 重注册也不会打乱前后分组）。
const entries = computed(() => {
  const ordered = [...views.value].sort(
    (a, b) => (a.source === 'builtin' ? 0 : 1) - (b.source === 'builtin' ? 0 : 1)
  );
  return ordered.map((v) => ({
    key: v.id,
    label: v.name,
    tip: v.tip,
    blocks: Math.min(4, Math.max(1, v.blocks ?? 2)),
    builtin: v.source === 'builtin',
  }));
});
</script>

<template>
  <div class="view-switch">
    <div v-for="mode in entries" :key="mode.key" class="mode-btn-wrap">
      <button :class="{ selected: viewMode === mode.key }" @click="viewMode = mode.key">{{ mode.label }}</button>
      <div class="mode-tip">
        <div class="mini" :class="mode.builtin ? `mini-${mode.key}` : 'mini-plugin'">
          <!-- 波形：流动的正弦曲线（内置 wave 与外置插件 waveform 视图均命中） -->
          <svg v-if="mode.key === 'wave' || mode.key.startsWith('plugin-wave-')" class="mini-wave" viewBox="0 0 120 32" preserveAspectRatio="none">
            <line x1="0" y1="5" x2="120" y2="5" class="grid" />
            <line x1="0" y1="16" x2="120" y2="16" class="grid" />
            <line x1="0" y1="27" x2="120" y2="27" class="grid" />
            <path
              class="sine"
              d="M0 16 C 10 2, 20 2, 30 16 S 50 30, 60 16 S 80 2, 90 16 S 110 30, 120 16"
            />
          </svg>
          <!-- 转发：数据包从源会话流向目标会话 -->
          <svg v-else-if="mode.key === 'forward'" class="mini-forward-svg" viewBox="0 0 120 32">
            <rect x="6" y="9" width="26" height="14" rx="3" class="node" />
            <rect x="88" y="9" width="26" height="14" rx="3" class="node" />
            <line x1="32" y1="16" x2="86" y2="16" class="wire" />
            <path d="M80 10 L 87 16 L 80 22" class="arrow" />
            <circle r="3" class="packet">
              <animateMotion dur="1.4s" repeatCount="indefinite" path="M34 16 L 84 16" />
            </circle>
          </svg>
          <!-- 其余视图：块状缩略 -->
          <template v-else>
            <i v-for="n in mode.blocks" :key="n" :style="{ animationDelay: `${(n - 1) * 0.4}s` }"></i>
          </template>
        </div>
        <p>{{ mode.tip }}</p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.view-switch {
  display: flex;
  flex-wrap: wrap; /* 插件视图增多/窗口收窄时按钮折行，条子整体不越出卡片边界 */
  max-width: 100%;
  gap: 2px;
  background: rgba(23, 26, 33, 0.06);
  border-radius: 8px;
  padding: 3px;
}

.view-switch button {
  padding: 6px 11px;
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

/* 总览：三行横条依次流动 */
.mini-global {
  grid-template-columns: 1fr;
}

.mini-global i {
  height: 6px;
}

/* 转发：源 → 目标（SVG 示意，数据包沿连线流动） */
.mini-forward-svg,
.mini-wave {
  display: block;
  width: 100%;
  height: 30px;
}

.mini-wave .grid {
  stroke: rgba(23, 26, 33, 0.14);
  stroke-width: 1;
}

.mini-wave .sine {
  fill: none;
  stroke: #3b6fd4;
  stroke-width: 2;
  stroke-linecap: round;
  stroke-dasharray: 9 5;
  animation: miniWaveFlow 1s linear infinite;
}

@keyframes miniWaveFlow {
  to {
    stroke-dashoffset: -14;
  }
}

.mini-forward-svg .node {
  fill: rgba(59, 111, 212, 0.18);
  stroke: #3b6fd4;
  stroke-width: 1.2;
}

.mini-forward-svg .wire {
  stroke: rgba(59, 111, 212, 0.4);
  stroke-width: 1.5;
}

.mini-forward-svg .arrow {
  fill: none;
  stroke: #3b6fd4;
  stroke-width: 2;
  stroke-linecap: round;
  stroke-linejoin: round;
}

.mini-forward-svg .packet {
  fill: #3b6fd4;
}

/* 插件视图：通用纵向条块缩略图 */
.mini-plugin {
  grid-template-columns: 1fr;
}

.mini-plugin i {
  height: 8px;
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

/* 深色：SVG 示意图配色 */
.theme-dark .mini-wave .grid {
  stroke: rgba(255, 255, 255, 0.16);
}

.theme-dark .mini-wave .sine {
  stroke: #6ca7e8;
}

.theme-dark .mini-forward-svg .node {
  fill: rgba(87, 157, 245, 0.22);
  stroke: #6ca7e8;
}

.theme-dark .mini-forward-svg .wire {
  stroke: rgba(87, 157, 245, 0.4);
}

.theme-dark .mini-forward-svg .arrow {
  stroke: #6ca7e8;
}

.theme-dark .mini-forward-svg .packet {
  fill: #8fbdf7;
}
</style>
