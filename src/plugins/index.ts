// 插件系统入口：模块加载即注册内置项（首帧前生效）；
// initPlugins() 在 App.vue onMounted 装载已安装的第三方插件（需要 Tauri 文件桥）。
import { registerBuiltins } from './builtin';
import { initPlugins } from './host';

registerBuiltins();

export { initPlugins };
