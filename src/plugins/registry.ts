// ==================================================================
// 注册表：内置视图/协议与插件注册项的统一存放处（响应式）。
// ViewSwitch 只读这里；波形等内置功能与第三方插件走同一条注册路径。
// ==================================================================
import { markRaw, shallowRef } from 'vue';
import type { ProtocolDef, ViewDef } from './types';

export interface ViewEntry extends ViewDef {
  source: string; // 'builtin' 或插件 id
}

export interface ProtocolEntry extends ProtocolDef {
  source: string;
}

// shallowRef + markRaw：组件对象绝不能进深层响应式代理——
// 深代理会让 <component :is> 的挂载/卸载经过 Proxy 层（性能劣化 + 卸载钩子异常）
export const views = shallowRef<ViewEntry[]>([]);
export const protocols = shallowRef<ProtocolEntry[]>([]);

// 同 id 覆盖注册（HMR/重载场景）；后注册的排在后面（插件视图在内置视图之后）
export const registerView = (entry: ViewEntry) => {
  const next = views.value.filter((v) => v.id !== entry.id);
  next.push({ ...entry, component: markRaw(entry.component as ViewDef['component']) as ViewDef['component'] });
  views.value = next;
};

export const unregisterBySource = (source: string) => {
  views.value = views.value.filter((v) => v.source !== source);
  protocols.value = protocols.value.filter((p) => p.source !== source);
};

export const registerProtocol = (entry: ProtocolEntry) => {
  // shallowRef 下必须整体重赋值——原地 push 不会触发依赖更新
  protocols.value = [...protocols.value.filter((p) => p.id !== entry.id), entry];
};

export const resolveView = (mode: string): ViewEntry | undefined => views.value.find((v) => v.id === mode);

export const findProtocol = (id: string): ProtocolEntry | undefined => protocols.value.find((p) => p.id === id);
