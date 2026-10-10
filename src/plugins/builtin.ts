// 内置注册：单屏/分屏/总览/转发四个经典视图。
// 波形已外置为市场插件（NNSerialTool-Plugins 仓库 wave），走与第三方插件
// 完全相同的安装链路；协议下拉中的 NN-Wave 也由该插件注册。
import DetailView from '../components/DetailView.vue';
import SplitView from '../components/SplitView.vue';
import BusView from '../components/BusView.vue';
import ForwardView from '../components/ForwardView.vue';
import { registerView } from './registry';

export const registerBuiltins = () => {
  registerView({
    id: 'detail',
    name: '单屏',
    tip: '单连接大视图，完整参数与独立收发区',
    blocks: 3,
    component: DetailView,
    source: 'builtin',
  });
  registerView({
    id: 'split',
    name: '分屏',
    tip: '勾选的多个连接并排显示，互不干扰',
    blocks: 4,
    component: SplitView,
    source: 'builtin',
  });
  registerView({
    id: 'global',
    name: '总览',
    tip: '已加入总览的会话按时间线汇成一条流',
    blocks: 3,
    component: BusView,
    source: 'builtin',
  });
  registerView({
    id: 'forward',
    name: '转发',
    tip: '配置会话之间的数据转发规则',
    blocks: 2,
    component: ForwardView,
    source: 'builtin',
  });
};
