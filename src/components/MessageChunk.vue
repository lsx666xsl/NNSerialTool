<script setup lang="ts">
import type { SessionMessage } from '../types';

// 虚拟滚动窗口行渲染：父组件只传入当前视口附近的行切片。
// 行的显示数据（hexText / logSeg / snap）全部在入库时算好，这里只做读取；
// 行内容不再变化，稳定 key + v-memo 可让旧行直接跳过 patch。
defineProps<{
  rows: SessionMessage[];
  hideMeta: boolean;
}>();

// 方向前缀是否显示（RX/TX 各自独立开关，按消息快照）
const showDirTag = (m: SessionMessage) =>
  m.direction === 'RX' ? (m.snap?.rx ?? true) : (m.snap?.tx ?? true);
</script>

<template>
  <div class="flow-chunk" :class="{ 'hide-meta': hideMeta }">
    <div
      v-for="message in rows"
      :key="message.id"
      v-memo="[message.id]"
      class="flow-line"
      :class="message.direction.toLowerCase()"
    >
      <div class="flow-meta">
        <span v-if="message.time" class="flow-time">{{ message.time }}</span>
        <span v-if="showDirTag(message)" class="flow-dir">{{ message.direction }}</span>
      </div>
      <div class="flow-text">
        <template v-if="message.logSeg">
          <span
            v-for="(seg, i) in message.logSeg"
            :key="i"
            :class="seg.cls ? ['lv-tag', 'lv-' + seg.cls] : undefined"
          >{{ seg.text }}</span>
        </template>
        <template v-else>{{ message.hexText ?? message.text }}</template>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* display: contents：块本身不产生盒子，行在父级窗口层内连续排列 */
.flow-chunk {
  display: contents;
}

.flow-line {
  display: flex;
  flex-direction: column;
  gap: 1px;
  padding: 2px 4px;
  border-radius: 3px;
  white-space: pre-wrap;
  word-break: break-all;
  line-height: 1.5;
  color: #23262b;
}

.flow-line:hover {
  background: rgba(23, 26, 33, 0.04);
}

/* 元信息行：时间戳 + 方向标签，小一号浅色 */
.flow-meta {
  display: flex;
  gap: 8px;
  font-size: 0.85em;
}

.flow-time {
  color: #7c828c;
  font-weight: 400;
}

.flow-dir {
  font-weight: 700;
}

.flow-line.rx .flow-dir {
  color: #2e8b45;
}

.flow-line.tx .flow-dir {
  color: #2b6cb0;
}

/* 日志等级徽标：行首 [INFO]/[ERROR] 等标签着色（浅色主题，与 RX/TX 方向色同族） */
.lv-tag {
  font-weight: 600;
  padding: 0 3px;
  border-radius: 3px;
}

.lv-trace {
  color: #8a9099;
  background: rgba(138, 144, 153, 0.14);
}

.lv-debug {
  color: #2b6cb0;
  background: rgba(43, 108, 176, 0.12);
}

.lv-info {
  color: #2e8b45;
  background: rgba(46, 139, 69, 0.12);
}

.lv-warn {
  color: #b7791f;
  background: rgba(183, 121, 31, 0.14);
}

.lv-error {
  color: #c53030;
  background: rgba(197, 48, 48, 0.12);
}

/* FATAL 最醒目：实心色块反白 */
.lv-fatal {
  color: #ffffff;
  background: #c53030;
}

/* 标签总开关：隐藏时间戳/RX/TX 前缀。只改类名，不触发消息行重渲染。 */
.flow-chunk.hide-meta .flow-meta {
  display: none;
}

/* 深色主题：深底浅字，方向色提亮 */
.theme-dark .flow-line {
  color: #d6d9de;
}

.theme-dark .flow-line:hover {
  background: rgba(255, 255, 255, 0.05);
}

.theme-dark .flow-time {
  color: #7c828c;
}

.theme-dark .flow-line.rx .flow-dir {
  color: #6bc97e;
}

.theme-dark .flow-line.tx .flow-dir {
  color: #6ca7e8;
}

/* 深色主题等级徽标：提亮一档 */
.theme-dark .lv-trace {
  color: #6f737a;
  background: rgba(255, 255, 255, 0.06);
}

.theme-dark .lv-debug {
  color: #6ca7e8;
  background: rgba(108, 167, 232, 0.14);
}

.theme-dark .lv-info {
  color: #6bc97e;
  background: rgba(107, 201, 126, 0.14);
}

.theme-dark .lv-warn {
  color: #f6c453;
  background: rgba(246, 196, 83, 0.14);
}

.theme-dark .lv-error {
  color: #f87171;
  background: rgba(248, 113, 113, 0.16);
}

.theme-dark .lv-fatal {
  color: #1a1b1e;
  background: #f87171;
}
</style>
