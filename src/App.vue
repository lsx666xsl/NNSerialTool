<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue';
import { appliedTheme, disposeApp, initApp, viewMode } from './stores/appStore';
import BusView from './components/BusView.vue';
import DetailView from './components/DetailView.vue';
import SessionForm from './components/SessionForm.vue';
import SessionList from './components/SessionList.vue';
import SettingsMenu from './components/SettingsMenu.vue';
import UpdateButton from './components/UpdateButton.vue';
import SplitView from './components/SplitView.vue';
import ViewSwitch from './components/ViewSwitch.vue';

// App.vue 只负责布局骨架与生命周期装配；
// 状态与动作集中在 stores/appStore.ts，界面拆分在 components/ 下。
onMounted(() => {
  void initApp();
});

onUnmounted(() => {
  disposeApp();
});
</script>

<template>
  <main class="workspace" :class="`theme-${appliedTheme}`">
    <aside class="sidebar">
      <SessionForm />
      <SessionList />
    </aside>

    <section class="main-area">
      <header class="toolbar">
        <div>
          <h1>工作台</h1>
        </div>
        <div class="toolbar-actions">
          <UpdateButton />
          <SettingsMenu />
          <ViewSwitch />
        </div>
      </header>

      <DetailView v-if="viewMode === 'detail'" />
      <SplitView v-else-if="viewMode === 'split'" />
      <BusView v-else />
    </section>
  </main>
</template>
