<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

// 串口基础信息来自 Rust 后端 serial_list_ports 命令，前端只负责展示和选择。
type SerialPortInfo = {
  portnum: string;
  portproduct: string;
};

// 这几个联合类型相当于“只能从固定值中选择”的枚举，能减少拼写错误。
// TCP/UDP 现在只是预留类型，后续接入网络模块时可以继续沿用同一套会话架构。
type ConnectionType = 'serial' | 'tcp_client' | 'tcp_server' | 'udp';
type ConnectionStatus = 'closed' | 'connected';
type ViewMode = 'detail' | 'split' | 'global';
type ThemeMode = 'apple-light' | 'codex-dark';
type MessageDirection = 'RX' | 'TX' | 'INFO';

// 串口连接参数单独抽出来，方便以后做“场景保存”时直接序列化成配置文件。
type SerialConfig = {
  port: string;
  baudRate: number;
  dataBits: number;
  parityBits: string;
  stopBits: number;
};

// 一个 ConnectionSession 就是一条独立连接，也就是界面左侧列表中的一项。
// 关键点：每个会话自己保存 sendText/receiveText/status，所以多个串口互不影响。
type ConnectionSession = {
  id: string;
  name: string;
  type: ConnectionType;
  status: ConnectionStatus;
  config: SerialConfig;
  sendText: string;
  receiveText: string;
  messageCount: number;
  statusMsg: string;
};

// 全局消息总线用于把所有连接的收发记录放到同一个时间线里。
// 它不是替代每个会话的 receiveText，而是额外提供“跨连接观察顺序”的视角。
type GlobalMessage = {
  id: string;
  time: string;
  sessionId: string;
  sessionName: string;
  direction: MessageDirection;
  text: string;
};

// ports/baudRates 这些是“下拉框选项”，来自 Rust 后端，多个会话共享同一份。
const ports = ref<SerialPortInfo[]>([]);
const baudRates = ref<number[]>([]);
const dataBits = ref<number[]>([]);
const parityBits = ref<string[]>([]);
const stopBits = ref<number[]>([]);

// newSessionConfig 只负责左侧“新建会话”表单，不代表已经打开的真实串口。
const newSessionConfig = ref<SerialConfig>({
  port: '',
  baudRate: 9600,
  dataBits: 8,
  parityBits: 'None',
  stopBits: 1,
});

// 这是整个多连接界面的核心状态：
// sessions 管连接本身，globalMessages 管总线日志，viewMode 决定右侧用哪种方式展示。
const sessions = ref<ConnectionSession[]>([]);
const globalMessages = ref<GlobalMessage[]>([]);
const activeSessionId = ref<string>('');
const selectedSessionIds = ref<string[]>([]);
const viewMode = ref<ViewMode>('detail');
// themeMode 只影响界面外观，不影响串口连接和消息数据。
const themeMode = ref<ThemeMode>('apple-light');

const showCustomBaud = ref(false);
const customBaudRate = ref('');
const busSessionFilter = ref('all');
const busDirectionFilter = ref<'all' | MessageDirection>('all');
const busKeyword = ref('');

let pollTimer: ReturnType<typeof setInterval>;
let unlistenSerialData: UnlistenFn | undefined;
let unlistenSerialDisconnect: UnlistenFn | undefined;

// computed 是 Vue 的派生状态：源数据变化时自动重新计算，模板里直接使用即可。
const activeSession = computed(() => sessions.value.find((item) => item.id === activeSessionId.value));
const splitSessions = computed(() => sessions.value.filter((item) => selectedSessionIds.value.includes(item.id)));

// 总线过滤不修改原始日志，只决定当前界面显示哪些消息。
const filteredGlobalMessages = computed(() => {
  const keyword = busKeyword.value.trim().toLowerCase();

  return globalMessages.value.filter((message) => {
    const matchSession = busSessionFilter.value === 'all' || message.sessionId === busSessionFilter.value;
    const matchDirection = busDirectionFilter.value === 'all' || message.direction === busDirectionFilter.value;
    const matchKeyword = !keyword || message.text.toLowerCase().includes(keyword) || message.sessionName.toLowerCase().includes(keyword);
    return matchSession && matchDirection && matchKeyword;
  });
});

const connectedCount = computed(() => sessions.value.filter((item) => item.status === 'connected').length);
const themeLabel = computed(() => (themeMode.value === 'apple-light' ? '浅色 Apple' : '深色 Codex'));
const nextThemeLabel = computed(() => (themeMode.value === 'apple-light' ? '切换深色 Codex' : '切换浅色 Apple'));

const toggleTheme = () => {
  themeMode.value = themeMode.value === 'apple-light' ? 'codex-dark' : 'apple-light';
};

const nowText = () => new Date().toLocaleTimeString('zh-CN', { hour12: false }) + `.${String(new Date().getMilliseconds()).padStart(3, '0')}`;

const decodeBytes = (data: number[]) => new TextDecoder().decode(new Uint8Array(data));

// 每次收发或状态变化都写一条总线消息。
// 这里限制最多保留 2000 条，避免串口高速输出时前端内存无限增长。
const appendGlobalMessage = (session: ConnectionSession, direction: MessageDirection, text: string) => {
  globalMessages.value.push({
    id: `${Date.now()}-${Math.random()}`,
    time: nowText(),
    sessionId: session.id,
    sessionName: session.name,
    direction,
    text,
  });

  if (globalMessages.value.length > 2000) {
    globalMessages.value.splice(0, globalMessages.value.length - 2000);
  }
};

const addCustomBaudRate = () => {
  const val = parseInt(customBaudRate.value, 10);
  if (isNaN(val) || val <= 0) return;

  if (!baudRates.value.includes(val)) {
    baudRates.value.push(val);
    baudRates.value.sort((a, b) => a - b);
  }

  newSessionConfig.value.baudRate = val;
  customBaudRate.value = '';
  showCustomBaud.value = false;
};

const refreshPorts = async () => {
  try {
    ports.value = await invoke<SerialPortInfo[]>('serial_list_ports');
    if (!newSessionConfig.value.port && ports.value.length > 0) {
      newSessionConfig.value.port = ports.value[0].portnum;
    }
  } catch (e) {
    console.error('获取串口失败:', e);
  }
};

const refreshOptions = async () => {
  baudRates.value = await invoke<number[]>('serial_baudrate_list');
  dataBits.value = await invoke<number[]>('serial_databit_list');
  parityBits.value = await invoke<string[]>('serial_paritybit_list');
  stopBits.value = await invoke<number[]>('serial_stopbit_list');
};

const sessionTypeLabel = (type: ConnectionType) => {
  const labels: Record<ConnectionType, string> = {
    serial: '串口',
    tcp_client: 'TCP Client',
    tcp_server: 'TCP Server',
    udp: 'UDP',
  };
  return labels[type];
};

// 创建会话只是把配置加入前端列表，不会马上占用串口。
// 用户点击“打开连接”时，才会通过 invoke 调用 Rust 后端真正打开硬件端口。
const createSerialSession = () => {
  if (!newSessionConfig.value.port) return;

  const samePortCount = sessions.value.filter((item) => item.config.port === newSessionConfig.value.port).length;
  const session: ConnectionSession = {
    id: `serial-${newSessionConfig.value.port}-${Date.now()}`,
    name: samePortCount === 0 ? newSessionConfig.value.port : `${newSessionConfig.value.port} #${samePortCount + 1}`,
    type: 'serial',
    status: 'closed',
    config: { ...newSessionConfig.value },
    sendText: '',
    receiveText: '',
    messageCount: 0,
    statusMsg: '已创建，等待打开',
  };

  sessions.value.push(session);
  activeSessionId.value = session.id;
  if (!selectedSessionIds.value.includes(session.id)) {
    selectedSessionIds.value.push(session.id);
  }
};

const removeSession = async (session: ConnectionSession) => {
  if (session.status === 'connected') {
    session.statusMsg = '请先关闭连接，再删除会话';
    appendGlobalMessage(session, 'INFO', session.statusMsg);
    return;
  }

  sessions.value = sessions.value.filter((item) => item.id !== session.id);
  selectedSessionIds.value = selectedSessionIds.value.filter((id) => id !== session.id);

  if (activeSessionId.value === session.id) {
    activeSessionId.value = sessions.value[0]?.id ?? '';
  }
};

const toggleSplitSession = (sessionId: string) => {
  if (selectedSessionIds.value.includes(sessionId)) {
    selectedSessionIds.value = selectedSessionIds.value.filter((id) => id !== sessionId);
  } else {
    selectedSessionIds.value.push(sessionId);
  }
};

// invoke 是前端调用 Rust 命令的桥梁。
// 参数名要和 Rust #[tauri::command] 函数的参数名对应，Tauri 会自动做驼峰/下划线转换。
const openSerial = async (session: ConnectionSession) => {
  if (session.type !== 'serial') {
    session.statusMsg = 'TCP/UDP 后端尚未实现，本阶段先预留入口';
    appendGlobalMessage(session, 'INFO', session.statusMsg);
    return;
  }

  try {
    const result = await invoke<string>('serial_open', {
      port: session.config.port,
      baudRate: session.config.baudRate,
      dataBits: session.config.dataBits,
      parityBits: session.config.parityBits,
      stopBits: session.config.stopBits,
    });
    session.status = 'connected';
    session.statusMsg = result;
    appendGlobalMessage(session, 'INFO', result);
  } catch (e) {
    session.statusMsg = `打开失败: ${e}`;
    appendGlobalMessage(session, 'INFO', session.statusMsg);
  }
};

const closeSerial = async (session: ConnectionSession) => {
  try {
    const result = await invoke<string>('serial_close', { port: session.config.port });
    session.status = 'closed';
    session.statusMsg = result;
    appendGlobalMessage(session, 'INFO', result);
  } catch (e) {
    session.statusMsg = `关闭失败: ${e}`;
    appendGlobalMessage(session, 'INFO', session.statusMsg);
  }
};

// 发送成功后也写入总线，这样总线能同时看到 TX 和 RX 的时间顺序。
const sendData = async (session: ConnectionSession) => {
  if (!session.sendText.trim()) return;

  if (session.status !== 'connected') {
    session.statusMsg = '请先打开连接再发送数据';
    appendGlobalMessage(session, 'INFO', session.statusMsg);
    return;
  }

  try {
    await invoke('serial_write', {
      port: session.config.port,
      data: session.sendText,
    });
    appendGlobalMessage(session, 'TX', session.sendText);
    session.messageCount += 1;
  } catch (e) {
    session.statusMsg = `发送失败: ${e}`;
    appendGlobalMessage(session, 'INFO', session.statusMsg);
  }
};

const clearSessionReceive = (session: ConnectionSession) => {
  session.receiveText = '';
  session.messageCount = 0;
};

const findSerialSessionByPort = (port: string) => sessions.value.find((item) => item.type === 'serial' && item.config.port === port);

// 修改已打开串口的参数时，前端状态和 Rust 后端硬件参数都要同步更新。
// 端口号本身不能在连接中途切换，所以界面上连接后会禁用端口选择。
const updateSerialConfig = async (session: ConnectionSession, key: keyof SerialConfig, value: string | number) => {
  if (key === 'port') {
    session.config.port = String(value);
    session.name = String(value);
    return;
  }

  if (key === 'baudRate') session.config.baudRate = Number(value);
  if (key === 'dataBits') session.config.dataBits = Number(value);
  if (key === 'parityBits') session.config.parityBits = String(value);
  if (key === 'stopBits') session.config.stopBits = Number(value);

  if (session.status !== 'connected') return;

  try {
    if (key === 'baudRate') await invoke('serial_set_baudrate', { port: session.config.port, baudRate: session.config.baudRate });
    if (key === 'dataBits') await invoke('serial_set_databits', { port: session.config.port, dataBits: session.config.dataBits });
    if (key === 'parityBits') await invoke('serial_set_parity', { port: session.config.port, parity: session.config.parityBits });
    if (key === 'stopBits') await invoke('serial_set_stopbits', { port: session.config.port, stopBits: session.config.stopBits });
    session.statusMsg = '连接参数已更新';
  } catch (e) {
    session.statusMsg = `修改参数失败: ${e}`;
    appendGlobalMessage(session, 'INFO', session.statusMsg);
  }
};

onMounted(async () => {
  await refreshOptions();
  await refreshPorts();

  // serial-read_data 是 Rust 读线程 emit 出来的事件。
  // payload 里带 port，所以前端能知道这段数据属于哪个会话。
  unlistenSerialData = await listen<{ port: string; data: number[] }>('serial-read_data', (event) => {
    const session = findSerialSessionByPort(event.payload.port);
    if (!session) return;

    const text = decodeBytes(event.payload.data);
    session.receiveText += text;
    session.messageCount += 1;
    appendGlobalMessage(session, 'RX', text);
  });

  unlistenSerialDisconnect = await listen<string>('serial-disconnect', (event) => {
    const session = findSerialSessionByPort(event.payload);
    if (!session) return;

    session.status = 'closed';
    session.statusMsg = `串口 ${event.payload} 已断开`;
    appendGlobalMessage(session, 'INFO', session.statusMsg);
  });

  pollTimer = setInterval(refreshPorts, 2000);
});

onUnmounted(() => {1
  // 组件卸载时清理定时器和事件监听，避免窗口热更新后重复监听同一个串口事件。
  clearInterval(pollTimer);
  unlistenSerialData?.();
  unlistenSerialDisconnect?.();
});
</script>

<template>
  <main class="workspace" :class="themeMode">
    <aside class="sidebar">
      <section class="panel create-panel">
        <div class="section-title">
          <h2>新建串口会话</h2>
          <button class="ghost-btn" @click="refreshPorts">刷新</button>
        </div>

        <div class="form-row">
          <label>端口号</label>
          <select v-model="newSessionConfig.port">
            <option value="" disabled>请选择串口</option>
            <option v-for="port in ports" :key="port.portnum" :value="port.portnum">
              {{ port.portnum }} - {{ port.portproduct }}
            </option>
          </select>
        </div>

        <div class="form-row">
          <label>波特率</label>
          <select v-model="newSessionConfig.baudRate">
            <option v-for="baudRate in baudRates" :key="baudRate" :value="baudRate">{{ baudRate }}</option>
          </select>
          <button v-if="!showCustomBaud" class="small-btn success-btn" @click="showCustomBaud = true">+</button>
          <input v-else v-model="customBaudRate" placeholder="自定义" @keyup.enter="addCustomBaudRate" @blur="addCustomBaudRate" />
        </div>

        <div class="form-row compact-row">
          <label>数据位</label>
          <select v-model="newSessionConfig.dataBits">
            <option v-for="dataBit in dataBits" :key="dataBit" :value="dataBit">{{ dataBit }}</option>
          </select>
        </div>

        <div class="form-row compact-row">
          <label>校验位</label>
          <select v-model="newSessionConfig.parityBits">
            <option v-for="parityBit in parityBits" :key="parityBit" :value="parityBit">{{ parityBit }}</option>
          </select>
        </div>

        <div class="form-row compact-row">
          <label>停止位</label>
          <select v-model="newSessionConfig.stopBits">
            <option v-for="stopBit in stopBits" :key="stopBit" :value="stopBit">{{ stopBit }}</option>
          </select>
        </div>

        <button class="primary-btn full-btn" :disabled="!newSessionConfig.port" @click="createSerialSession">添加串口会话</button>
        <p class="hint">TCP/UDP 会在后续阶段接入，当前先完成多串口架构。</p>
      </section>

      <section class="panel session-panel">
        <div class="section-title">
          <h2>连接列表</h2>
          <span>{{ connectedCount }}/{{ sessions.length }} 已连接</span>
        </div>

        <div v-if="sessions.length === 0" class="empty-box">还没有连接会话，请先添加一个串口。</div>

        <div
          v-for="session in sessions"
          :key="session.id"
          class="session-card"
          :class="{ active: session.id === activeSessionId }"
          @click="activeSessionId = session.id"
        >
          <div class="session-main">
            <div>
              <strong>{{ session.name }}</strong>
              <p>{{ sessionTypeLabel(session.type) }} · {{ session.config.baudRate }}bps</p>
            </div>
            <span class="status-dot" :class="session.status"></span>
          </div>
          <div class="session-actions">
            <label class="check-label" @click.stop>
              <input type="checkbox" :checked="selectedSessionIds.includes(session.id)" @change="toggleSplitSession(session.id)" />
              分屏
            </label>
            <span>{{ session.messageCount }} 条</span>
            <button class="danger-text" @click.stop="removeSession(session)">删除</button>
          </div>
        </div>
      </section>
    </aside>

    <section class="main-area">
      <header class="toolbar">
        <div>
          <h1>多连接调试工作台</h1>
          <p>详情、分屏、总线三种视图共享同一份连接数据，切换视图不会断开连接。</p>
        </div>
        <div class="toolbar-actions">
          <button class="theme-toggle" @click="toggleTheme">
            <span>{{ themeLabel }}</span>
            <strong>{{ nextThemeLabel }}</strong>
          </button>
          <div class="view-switch">
            <button :class="{ selected: viewMode === 'detail' }" @click="viewMode = 'detail'">详情</button>
            <button :class="{ selected: viewMode === 'split' }" @click="viewMode = 'split'">分屏</button>
            <button :class="{ selected: viewMode === 'global' }" @click="viewMode = 'global'">总线</button>
          </div>
        </div>
      </header>

      <section v-if="viewMode === 'detail'" class="content-card detail-view">
        <div v-if="!activeSession" class="empty-box large">请选择或新建一个连接会话。</div>

        <template v-else>
          <div class="detail-header">
            <div>
              <h2>{{ activeSession.name }}</h2>
              <p>{{ sessionTypeLabel(activeSession.type) }} · {{ activeSession.statusMsg }}</p>
            </div>
            <div class="button-group">
              <button v-if="activeSession.status === 'closed'" class="primary-btn" @click="openSerial(activeSession)">打开连接</button>
              <button v-else class="danger-btn" @click="closeSerial(activeSession)">关闭连接</button>
            </div>
          </div>

          <div class="config-grid">
            <label>
              端口号
              <select :value="activeSession.config.port" :disabled="activeSession.status === 'connected'" @change="updateSerialConfig(activeSession, 'port', ($event.target as HTMLSelectElement).value)">
                <option v-for="port in ports" :key="port.portnum" :value="port.portnum">{{ port.portnum }}</option>
              </select>
            </label>
            <label>
              波特率
              <select :value="activeSession.config.baudRate" @change="updateSerialConfig(activeSession, 'baudRate', Number(($event.target as HTMLSelectElement).value))">
                <option v-for="baudRate in baudRates" :key="baudRate" :value="baudRate">{{ baudRate }}</option>
              </select>
            </label>
            <label>
              数据位
              <select :value="activeSession.config.dataBits" @change="updateSerialConfig(activeSession, 'dataBits', Number(($event.target as HTMLSelectElement).value))">
                <option v-for="dataBit in dataBits" :key="dataBit" :value="dataBit">{{ dataBit }}</option>
              </select>
            </label>
            <label>
              校验位
              <select :value="activeSession.config.parityBits" @change="updateSerialConfig(activeSession, 'parityBits', ($event.target as HTMLSelectElement).value)">
                <option v-for="parityBit in parityBits" :key="parityBit" :value="parityBit">{{ parityBit }}</option>
              </select>
            </label>
            <label>
              停止位
              <select :value="activeSession.config.stopBits" @change="updateSerialConfig(activeSession, 'stopBits', Number(($event.target as HTMLSelectElement).value))">
                <option v-for="stopBit in stopBits" :key="stopBit" :value="stopBit">{{ stopBit }}</option>
              </select>
            </label>
          </div>

          <div class="io-grid">
            <div class="io-panel">
              <div class="panel-title">
                <h3>发送</h3>
                <span>TX</span>
              </div>
              <textarea v-model="activeSession.sendText" placeholder="输入要发送的数据"></textarea>
              <button class="primary-btn" @click="sendData(activeSession)">发送</button>
            </div>

            <div class="io-panel">
              <div class="panel-title">
                <h3>接收</h3>
                <button class="ghost-btn" @click="clearSessionReceive(activeSession)">清空</button>
              </div>
              <textarea v-model="activeSession.receiveText" readonly placeholder="等待接收数据"></textarea>
            </div>
          </div>
        </template>
      </section>

      <section v-else-if="viewMode === 'split'" class="content-card split-view">
        <div v-if="splitSessions.length === 0" class="empty-box large">请在左侧连接列表勾选要分屏显示的会话。</div>

        <div v-else class="split-grid" :class="`count-${Math.min(splitSessions.length, 4)}`">
          <article v-for="session in splitSessions" :key="session.id" class="split-card">
            <div class="panel-title">
              <div>
                <h3>{{ session.name }}</h3>
                <p>{{ session.status === 'connected' ? '已连接' : '未连接' }}</p>
              </div>
              <span class="status-dot" :class="session.status"></span>
            </div>
            <textarea v-model="session.receiveText" readonly></textarea>
            <div class="split-send">
              <input v-model="session.sendText" placeholder="发送数据" @keyup.enter="sendData(session)" />
              <button class="primary-btn" @click="sendData(session)">发送</button>
            </div>
          </article>
        </div>
      </section>

      <section v-else class="content-card global-view">
        <div class="filter-bar">
          <label>
            连接
            <select v-model="busSessionFilter">
              <option value="all">全部</option>
              <option v-for="session in sessions" :key="session.id" :value="session.id">{{ session.name }}</option>
            </select>
          </label>
          <label>
            方向
            <select v-model="busDirectionFilter">
              <option value="all">全部</option>
              <option value="RX">RX</option>
              <option value="TX">TX</option>
              <option value="INFO">INFO</option>
            </select>
          </label>
          <label>
            关键字
            <input v-model="busKeyword" placeholder="搜索内容或连接名" />
          </label>
          <button class="ghost-btn" @click="globalMessages = []">清空总线</button>
        </div>

        <div class="message-list">
          <div v-if="filteredGlobalMessages.length === 0" class="empty-box">暂无匹配消息。</div>
          <div v-for="message in filteredGlobalMessages" :key="message.id" class="message-line" :class="message.direction.toLowerCase()">
            <span class="time">{{ message.time }}</span>
            <span class="source">{{ message.sessionName }}</span>
            <span class="direction">{{ message.direction }}</span>
            <span class="payload">{{ message.text }}</span>
          </div>
        </div>
      </section>
    </section>
  </main>
</template>

<style>
html,
body,
#app {
  height: 100%;
  margin: 0;
  overflow: hidden;
}

* {
  box-sizing: border-box;
}
</style>

<style scoped>
.workspace {
  display: flex;
  height: 100%;
  background:
    radial-gradient(circle at 16% 12%, rgba(59, 130, 246, 0.16), transparent 26%),
    radial-gradient(circle at 84% 8%, rgba(168, 85, 247, 0.12), transparent 24%),
    #f5f5f7;
  color: #111827;
  font-family: Inter, -apple-system, BlinkMacSystemFont, 'Segoe UI', Arial, Helvetica, sans-serif;
  transition: background 0.24s ease, color 0.24s ease;
}
.sidebar {
  width: 360px;
  padding: 20px;
  display: flex;
  flex-direction: column;
  gap: 18px;
  overflow: auto;
}

.main-area {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  padding: 20px 20px 20px 0;
  gap: 18px;
}

.panel,
.content-card {
  background: rgba(255, 255, 255, 0.82);
  border: 1px solid rgba(255, 255, 255, 0.72);
  border-radius: 24px;
  box-shadow: 0 24px 70px rgba(15, 23, 42, 0.12);
  backdrop-filter: blur(22px);
}

.panel {
  padding: 18px;
}

.section-title,
.toolbar,
.detail-header,
.panel-title,
.session-main,
.session-actions,
.filter-bar,
.split-send,
.button-group {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.toolbar-actions {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
  justify-content: flex-end;
}

.theme-toggle {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 2px;
  min-width: 148px;
  background: rgba(255, 255, 255, 0.68);
  color: #111827;
  box-shadow: inset 0 0 0 1px rgba(148, 163, 184, 0.18), 0 10px 26px rgba(15, 23, 42, 0.08);
}

.theme-toggle span {
  font-size: 12px;
  color: #64748b;
  font-weight: 600;
}

.theme-toggle strong {
  font-size: 13px;
}

h1,
h2,
h3,
p {
  margin: 0;
}

h1 {
  font-size: 24px;
}

h2 {
  font-size: 18px;
}

h3 {
  font-size: 16px;
}

p,
.hint,
.session-actions,
.toolbar p {
  color: #6b7280;
  font-size: 13px;
}

.create-panel,
.session-panel {
  flex-shrink: 0;
}

.form-row {
  display: grid;
  grid-template-columns: 72px 1fr auto;
  align-items: center;
  gap: 8px;
  margin-top: 12px;
}

.compact-row {
  grid-template-columns: 72px 1fr;
}

label {
  font-size: 14px;
  color: #374151;
}

input,
select,
textarea {
  width: 100%;
  border: 1px solid rgba(148, 163, 184, 0.28);
  border-radius: 14px;
  padding: 10px 12px;
  font: inherit;
  background: rgba(255, 255, 255, 0.88);
  outline: none;
  transition: border-color 0.18s ease, box-shadow 0.18s ease, background 0.18s ease;
}

input:focus,
select:focus,
textarea:focus {
  border-color: rgba(37, 99, 235, 0.58);
  box-shadow: 0 0 0 4px rgba(37, 99, 235, 0.12);
  background: #ffffff;
}

textarea {
  resize: none;
  min-height: 160px;
  font-family: Consolas, 'Courier New', monospace;
  line-height: 1.5;
}

button {
  border: none;
  border-radius: 999px;
  padding: 10px 16px;
  cursor: pointer;
  font-weight: 700;
  transition: transform 0.16s ease, box-shadow 0.16s ease, background 0.16s ease;
}

button:hover:not(:disabled) {
  transform: translateY(-1px);
}

button:disabled {
  cursor: not-allowed;
  opacity: 0.5;
}

.primary-btn {
  background: linear-gradient(135deg, #2563eb, #4f46e5);
  color: #ffffff;
  box-shadow: 0 10px 24px rgba(37, 99, 235, 0.26);
}

.primary-btn:hover:not(:disabled) {
  background: #1d4ed8;
}

.danger-btn {
  background: linear-gradient(135deg, #ef4444, #f97316);
  color: #ffffff;
  box-shadow: 0 10px 24px rgba(239, 68, 68, 0.22);
}

.danger-btn:hover {
  background: #dc2626;
}

.ghost-btn {
  background: rgba(238, 242, 255, 0.86);
  color: #3730a3;
  box-shadow: inset 0 0 0 1px rgba(99, 102, 241, 0.1);
}

.success-btn {
  background: #22c55e;
  color: #ffffff;
}

.small-btn {
  width: 34px;
  padding: 8px 0;
}

.full-btn {
  width: 100%;
  margin-top: 14px;
}

.danger-text {
  background: transparent;
  color: #dc2626;
  padding: 0;
}

.session-card {
  border: 1px solid rgba(148, 163, 184, 0.18);
  border-radius: 18px;
  padding: 14px;
  margin-top: 12px;
  cursor: pointer;
  background: rgba(255, 255, 255, 0.64);
  transition: transform 0.18s ease, border-color 0.18s ease, box-shadow 0.18s ease, background 0.18s ease;
}

.session-card:hover {
  transform: translateY(-1px);
  border-color: rgba(37, 99, 235, 0.28);
  box-shadow: 0 12px 30px rgba(15, 23, 42, 0.08);
}

.session-card.active {
  border-color: rgba(37, 99, 235, 0.55);
  background: linear-gradient(135deg, rgba(239, 246, 255, 0.96), rgba(245, 243, 255, 0.9));
  box-shadow: 0 16px 38px rgba(37, 99, 235, 0.14);
}

.session-main p {
  margin-top: 4px;
}

.status-dot {
  width: 12px;
  height: 12px;
  border-radius: 999px;
  background: #9ca3af;
  flex-shrink: 0;
}

.status-dot.connected {
  background: #22c55e;
}

.status-dot.closed {
  background: #9ca3af;
}

.check-label {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.check-label input {
  width: auto;
}

.toolbar {
  background: rgba(255, 255, 255, 0.82);
  border: 1px solid rgba(255, 255, 255, 0.72);
  border-radius: 24px;
  padding: 18px 22px;
  box-shadow: 0 24px 70px rgba(15, 23, 42, 0.12);
  backdrop-filter: blur(22px);
}

.view-switch {
  display: flex;
  background: rgba(243, 244, 246, 0.86);
  border-radius: 999px;
  padding: 5px;
  box-shadow: inset 0 0 0 1px rgba(148, 163, 184, 0.18);
}

.view-switch button {
  background: transparent;
  color: #4b5563;
}

.view-switch button.selected {
  background: linear-gradient(135deg, #111827, #2563eb);
  color: #ffffff;
  box-shadow: 0 10px 24px rgba(37, 99, 235, 0.24);
}

.content-card {
  flex: 1;
  min-height: 0;
  padding: 22px;
  overflow: hidden;
}

.detail-view,
.global-view,
.split-view {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.config-grid {
  display: grid;
  grid-template-columns: repeat(5, minmax(120px, 1fr));
  gap: 12px;
}

.config-grid label {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.io-grid {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 16px;
}

.io-panel,
.split-card {
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
  background: rgba(249, 250, 251, 0.76);
  border: 1px solid rgba(148, 163, 184, 0.18);
  border-radius: 20px;
  padding: 16px;
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.72);
}

.io-panel textarea,
.split-card textarea {
  flex: 1;
  min-height: 0;
}

.split-grid {
  flex: 1;
  min-height: 0;
  display: grid;
  gap: 14px;
  overflow: auto;
}

.split-grid.count-1 {
  grid-template-columns: 1fr;
}

.split-grid.count-2,
.split-grid.count-3,
.split-grid.count-4 {
  grid-template-columns: repeat(2, minmax(280px, 1fr));
}

.split-card {
  min-height: 260px;
}

.filter-bar {
  justify-content: flex-start;
  flex-wrap: wrap;
}

.filter-bar label {
  min-width: 160px;
}

.message-list {
  flex: 1;
  min-height: 0;
  overflow: auto;
  border: 1px solid rgba(148, 163, 184, 0.22);
  border-radius: 20px;
  background: #0f172a;
  padding: 12px;
  font-family: Consolas, 'Courier New', monospace;
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.04);
}

.message-line {
  display: grid;
  grid-template-columns: 110px 130px 52px 1fr;
  gap: 10px;
  padding: 7px 10px;
  border-radius: 12px;
  color: #e5e7eb;
  white-space: pre-wrap;
  word-break: break-all;
}

.message-line.rx .direction {
  color: #22c55e;
}

.message-line.tx .direction {
  color: #60a5fa;
}

.message-line.info .direction {
  color: #fbbf24;
}

.time,
.source {
  color: #9ca3af;
}

.empty-box {
  padding: 20px;
  border: 1px dashed rgba(148, 163, 184, 0.46);
  border-radius: 20px;
  color: #64748b;
  text-align: center;
  background: rgba(248, 250, 252, 0.72);
}

.empty-box.large {
  flex: 1;
  display: grid;
  place-items: center;
}


.workspace.codex-dark {
  background:
    radial-gradient(circle at 18% 10%, rgba(59, 130, 246, 0.2), transparent 28%),
    radial-gradient(circle at 82% 12%, rgba(139, 92, 246, 0.18), transparent 26%),
    linear-gradient(135deg, #020617, #0f172a 48%, #111827);
  color: #e5e7eb;
}

.codex-dark .panel,
.codex-dark .content-card,
.codex-dark .toolbar {
  background: rgba(15, 23, 42, 0.76);
  border-color: rgba(148, 163, 184, 0.18);
  box-shadow: 0 24px 80px rgba(0, 0, 0, 0.38);
}

.codex-dark h1,
.codex-dark h2,
.codex-dark h3,
.codex-dark strong {
  color: #f8fafc;
}

.codex-dark p,
.codex-dark .hint,
.codex-dark .session-actions,
.codex-dark .toolbar p,
.codex-dark label,
.codex-dark .time,
.codex-dark .source {
  color: #94a3b8;
}

.codex-dark input,
.codex-dark select,
.codex-dark textarea {
  color: #e5e7eb;
  background: rgba(2, 6, 23, 0.58);
  border-color: rgba(148, 163, 184, 0.22);
}

.codex-dark input::placeholder,
.codex-dark textarea::placeholder {
  color: #64748b;
}

.codex-dark input:focus,
.codex-dark select:focus,
.codex-dark textarea:focus {
  background: rgba(15, 23, 42, 0.94);
  border-color: rgba(96, 165, 250, 0.72);
  box-shadow: 0 0 0 4px rgba(59, 130, 246, 0.16);
}

.codex-dark .session-card,
.codex-dark .io-panel,
.codex-dark .split-card,
.codex-dark .empty-box {
  background: rgba(15, 23, 42, 0.56);
  border-color: rgba(148, 163, 184, 0.16);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.04);
}

.codex-dark .session-card:hover {
  border-color: rgba(96, 165, 250, 0.36);
  box-shadow: 0 14px 34px rgba(0, 0, 0, 0.28);
}

.codex-dark .session-card.active {
  background: linear-gradient(135deg, rgba(30, 41, 59, 0.9), rgba(30, 64, 175, 0.36));
  border-color: rgba(96, 165, 250, 0.58);
  box-shadow: 0 18px 44px rgba(37, 99, 235, 0.18);
}

.codex-dark .view-switch {
  background: rgba(2, 6, 23, 0.62);
  box-shadow: inset 0 0 0 1px rgba(148, 163, 184, 0.16);
}

.codex-dark .view-switch button {
  color: #94a3b8;
}

.codex-dark .view-switch button.selected {
  background: linear-gradient(135deg, #2563eb, #7c3aed);
  color: #ffffff;
}

.codex-dark .theme-toggle,
.codex-dark .ghost-btn {
  background: rgba(30, 41, 59, 0.82);
  color: #dbeafe;
  box-shadow: inset 0 0 0 1px rgba(96, 165, 250, 0.14);
}

.codex-dark .theme-toggle span {
  color: #93c5fd;
}

.codex-dark .message-list {
  background: rgba(2, 6, 23, 0.86);
  border-color: rgba(148, 163, 184, 0.2);
}

.codex-dark .message-line {
  color: #e2e8f0;
}

.codex-dark .danger-text {
  color: #fca5a5;
}

@media (max-width: 1100px) {
  .workspace {
    flex-direction: column;
    overflow: auto;
  }

  .sidebar,
  .main-area {
    width: 100%;
    padding: 12px;
  }

  .io-grid,
  .config-grid,
  .split-grid.count-2,
  .split-grid.count-3,
  .split-grid.count-4 {
    grid-template-columns: 1fr;
  }
}
</style>
