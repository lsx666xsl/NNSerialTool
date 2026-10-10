import { createApp } from "vue";
import App_entryce from "./App.vue";
import "./styles/main.css";
import { startStallWatchdog } from "./debug/stall-watchdog";

startStallWatchdog(); // dev 构建专用：主线程卡死看门狗（Worker 侧输出卡死前面包屑）
createApp(App_entryce).mount("#app");
