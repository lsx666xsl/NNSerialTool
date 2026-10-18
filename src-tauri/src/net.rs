use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/*
*   描述：UDP 读线程的轮询超时时间。UDP 没有 shutdown 语义，只能靠读超时定期醒来
*         检查停止标志；TCP 读线程是阻塞读，由 shutdown(Both) 唤醒，不需要轮询。
*/
const POLL_INTERVAL_MS: u64 = 100;

/*
*   描述：TCP 连接单个地址的超时时间（域名可能解析出多个 IPv4/IPv6 地址，逐个尝试）
*/
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/*
*   描述：网络连接类型
*/
#[derive(Clone, Copy, PartialEq)]
pub enum NetKind {
    TcpClient,
    TcpServer,
    Udp,
}

impl NetKind {
    // 从前端传入的字符串解析连接类型
    fn parse(s: &str) -> Option<NetKind> {
        match s {
            "tcp_client" => Some(NetKind::TcpClient),
            "tcp_server" => Some(NetKind::TcpServer),
            "udp" => Some(NetKind::Udp),
            _ => None,
        }
    }
}

/*
*   描述：TCP 服务端已接入的一条客户端连接
*/
struct TcpClientConn {
    addr: SocketAddr,
    stream: Arc<Mutex<TcpStream>>,
}

/*
*   描述：网络连接句柄，三种网络形态统一保存
*/
pub struct NetHandle {
    kind: NetKind,
    label: String,
    stop_flag: Arc<AtomicBool>,
    stream: Option<Arc<Mutex<TcpStream>>>, // TCP 客户端使用
    clients: Option<Arc<Mutex<Vec<TcpClientConn>>>>, // TCP 服务端使用
    socket: Option<Arc<UdpSocket>>,        // UDP 使用（send 为 &self，无需互斥锁）
}

impl NetHandle {
    /*
     *   描述：真正关闭底层 IO：置停止标志 + shutdown 套接字。
     *         TCP 客户端/服务端逐条 shutdown(Both)，阻塞中的 read 立即返回，读线程随即退出；
     *         UDP 没有 shutdown 语义，仅置标志，读线程靠 100ms 读超时醒来退出。
     */
    fn shutdown(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        match self.kind {
            NetKind::TcpClient => {
                if let Some(stream) = &self.stream {
                    if let Ok(stream) = stream.lock() {
                        let _ = stream.shutdown(Shutdown::Both);
                    }
                }
            }
            NetKind::TcpServer => {
                if let Some(clients) = &self.clients {
                    if let Ok(list) = clients.lock() {
                        for conn in list.iter() {
                            if let Ok(stream) = conn.stream.lock() {
                                let _ = stream.shutdown(Shutdown::Both);
                            }
                        }
                    }
                }
            }
            NetKind::Udp => {}
        }
    }

    /*
     *   描述：锁内只 clone 出写目标，实际写操作在锁外进行（不持连接状态表锁做 IO）
     */
    fn write_targets(&self) -> WriteTargets {
        WriteTargets {
            kind: self.kind,
            label: self.label.clone(),
            stream: self.stream.clone(),
            clients: self.clients.clone(),
            socket: self.socket.clone(),
        }
    }
}

/*
*   描述：写目标快照（从 NetHandle 克隆而来，脱离状态表锁后使用）
*/
struct WriteTargets {
    kind: NetKind,
    label: String,
    stream: Option<Arc<Mutex<TcpStream>>>,
    clients: Option<Arc<Mutex<Vec<TcpClientConn>>>>,
    socket: Option<Arc<UdpSocket>>,
}

/*
*   描述：全局网络连接哈希表（key = 前端会话 id）
*/
pub struct NetState {
    pub conns: HashMap<String, NetHandle>,
    // 自动发送任务表：key -> 停止标志（Rust 线程直接循环写，绕过前端 IPC）
    pub auto_sends: HashMap<String, Arc<AtomicBool>>,
    // 正在打开中的连接（占位）：防止并发 net_open 造成句柄覆盖泄漏
    pub opening: HashSet<String>,
}

/*
*   描述：全局共享网络状态
*/
pub type SharedNetState = Arc<Mutex<NetState>>;

/*
*   描述：默认生成的网络哈希表
*/
impl Default for NetState {
    fn default() -> Self {
        Self {
            conns: HashMap::new(),
            auto_sends: HashMap::new(),
            opening: HashSet::new(),
        }
    }
}

/*
*   描述：网络收发数据事件，from 为来源地址（无来源信息时为空字符串）
*/
#[derive(Serialize, Clone)]
struct NetDataEvent {
    key: String,
    data: Vec<u8>,
    from: String,
}

/*
*   描述：自动发送通知事件（前端据此记录 TX）
*/
#[derive(Serialize, Clone)]
struct AutoSentEvent {
    key: String,
    data: Vec<u8>,
}

/*
*   描述：网络写入的公共实现（发送命令与自动发送线程共用）
*   返回：成功写入的目标数：TCP 客户端 1、UDP 1、TCP 服务端为成功写入的客户端数（0 视为失败）
*   说明：调用方不能持连接状态表锁；TCP 服务端在锁内只做客户端快照，放锁后逐个写
*/
fn net_write_bytes(targets: &WriteTargets, bytes: &[u8]) -> Result<usize, String> {
    match targets.kind {
        NetKind::TcpClient => {
            let stream = targets.stream.as_ref().ok_or("句柄异常: 缺少 TCP 流")?;
            let mut sp = stream
                .lock()
                .map_err(|_| format!("发送失败，连接 {} 已断开", targets.label))?;
            sp.write_all(bytes)
                .map_err(|_| format!("发送失败，连接 {} 已断开", targets.label))?;
            Ok(1)
        }
        NetKind::TcpServer => {
            let list_arc = targets.clients.as_ref().ok_or("句柄异常: 缺少客户端列表")?;
            // 锁内只做快照（clone 每条客户端流），放锁后再写，避免写超时拖住接入/读线程
            let snapshot: Vec<(SocketAddr, Arc<Mutex<TcpStream>>)> = {
                let list = list_arc.lock().map_err(|e| e.to_string())?;
                list.iter()
                    .map(|conn| (conn.addr, conn.stream.clone()))
                    .collect()
            };
            if snapshot.is_empty() {
                return Err(format!("{} 暂无客户端接入", targets.label));
            }

            let mut sent = 0usize;
            let mut dead: Vec<SocketAddr> = Vec::new();
            for (addr, stream) in snapshot {
                let ok = match stream.lock() {
                    Ok(mut sp) => sp.write_all(bytes).is_ok(),
                    Err(_) => false,
                };
                if ok {
                    sent += 1;
                } else {
                    dead.push(addr);
                }
            }
            // 写失败的客户端已断开：从列表剔除（读线程读到 EOF 时也会剔除）
            if !dead.is_empty() {
                if let Ok(mut list) = list_arc.lock() {
                    list.retain(|conn| !dead.contains(&conn.addr));
                }
            }
            if sent == 0 {
                Err(format!("{} 暂无可用客户端（发送失败）", targets.label))
            } else {
                Ok(sent)
            }
        }
        NetKind::Udp => {
            let socket = targets.socket.as_ref().ok_or("句柄异常: 缺少 UDP 套接字")?;
            // Windows：目标无监听时 send 报 ConnectionReset（ICMP 回报），忽略继续尝试
            match socket.send(bytes) {
                Ok(_) => Ok(1),
                Err(e) if e.kind() == ErrorKind::ConnectionReset => Ok(1),
                Err(_) => Err(format!("发送失败，请检查 {} 的目标地址设置", targets.label)),
            }
        }
    }
}

/*
*   描述：判断 IO 错误是否为轮询超时（非致命，继续循环即可）
*/
fn is_poll_timeout(e: &std::io::Error) -> bool {
    e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut
}

/*
*   描述：网络数据统一上报给前端
*/
fn emit_net_data(app_handle: &AppHandle, key: &str, data: &[u8], from: &str) {
    let _ = app_handle.emit(
        "net-read_data",
        NetDataEvent {
            key: key.to_string(),
            data: data.to_vec(),
            from: from.to_string(),
        },
    );
}

/*
*   描述：连接死亡时的处理。独立客户端会上报 net-disconnect 让前端标记断开；
*         服务端子连接只从客户端列表剔除，不影响整个会话。
*/
fn on_conn_dead(
    key: &str,
    addr: SocketAddr,
    clients: &Option<Arc<Mutex<Vec<TcpClientConn>>>>,
    app_handle: &AppHandle,
) {
    match clients {
        Some(list) => list.lock().unwrap().retain(|c| c.addr != addr),
        None => {
            let _ = app_handle.emit("net-disconnect", key);
        }
    }
}

/*
*   描述：TCP 读线程，独立客户端与服务端子连接共用。
*         阻塞读：数据到达内核即返回；停止/关闭由写路径的 shutdown(Both) 唤醒，不需要轮询。
*/
fn tcp_read_thread(
    key: String,
    addr: SocketAddr,
    mut rd: TcpStream,
    clients: Option<Arc<Mutex<Vec<TcpClientConn>>>>,
    stop_flag: Arc<AtomicBool>,
    app_handle: AppHandle,
) {
    thread::spawn(move || {
        let mut rbuf = [0u8; 4096];
        loop {
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }

            match rd.read(&mut rbuf) {
                Ok(0) => {
                    // 对端正常关闭
                    on_conn_dead(&key, addr, &clients, &app_handle);
                    break;
                }
                Ok(n) => {
                    emit_net_data(&app_handle, &key, &rbuf[..n], &addr.to_string());
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => {
                    // 被信号中断，重试
                    continue;
                }
                Err(_) => {
                    on_conn_dead(&key, addr, &clients, &app_handle);
                    break;
                }
            }
        }
    });
}

/*
*   描述：TCP 服务端接受连接线程，每接入一个客户端就派生一个独立读线程。
*         非阻塞 accept + 20ms 轮询：关闭后最迟 20ms 退出并释放监听端口。
*/
fn tcp_accept_thread(
    key: String,
    listener: TcpListener,
    clients: Arc<Mutex<Vec<TcpClientConn>>>,
    stop_flag: Arc<AtomicBool>,
    app_handle: AppHandle,
) {
    thread::spawn(move || {
        loop {
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }

            match listener.accept() {
                Ok((stream, addr)) => {
                    let _ = stream.set_write_timeout(Some(Duration::from_millis(1000)));
                    // 读改阻塞模式（无 read timeout），shutdown 唤醒；Nagle 禁用小包立即发
                    let _ = stream.set_nodelay(true);
                    let shared = Arc::new(Mutex::new(stream));
                    clients.lock().unwrap().push(TcpClientConn {
                        addr,
                        stream: shared.clone(),
                    });

                    // 每个客户端一个独立读线程（独占 clone 句柄，与写锁分离）
                    let rd_result = shared.lock().unwrap().try_clone();
                    if let Ok(rd) = rd_result {
                        tcp_read_thread(
                            key.clone(),
                            addr,
                            rd,
                            Some(clients.clone()),
                            stop_flag.clone(),
                            app_handle.clone(),
                        );
                    }
                }
                Err(_) => {
                    // 非阻塞模式下没有新连接，稍作等待避免空转
                    thread::sleep(Duration::from_millis(20));
                }
            }
        }
    });
}

/*
*   描述：UDP 读线程，recv_from 轮询并把来源地址一并上报
*         读线程使用独立 clone 套接字，recv_from 阻塞等待时不与写路径互相影响
*/
fn udp_read_thread(
    key: String,
    socket: Arc<UdpSocket>,
    stop_flag: Arc<AtomicBool>,
    app_handle: AppHandle,
) {
    thread::spawn(move || {
        // 65535：UDP 数据报最大长度，避免大数据报被小缓冲静默截断
        let mut rbuf = [0u8; 65535];
        loop {
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }

            match socket.recv_from(&mut rbuf) {
                Ok((n, addr)) => {
                    emit_net_data(&app_handle, &key, &rbuf[..n], &addr.to_string());
                }
                Err(e) => {
                    // Windows UDP：向无监听目标发送后内核回报 ICMP 不可达，recv_from 持续报 ConnectionReset——正常现象而非断线
                    let is_reset = e.kind() == ErrorKind::ConnectionReset;
                    if is_poll_timeout(&e) || is_reset || e.kind() == ErrorKind::Interrupted {
                        continue;
                    }
                    let _ = app_handle.emit("net-disconnect", &key);
                    break;
                }
            }
        }
    });
}

/*
*   描述：新建连接时线程启动需要的资源（句柄登记到状态表之后再启动线程）
*/
enum NetReadSetup {
    TcpClient {
        rd: TcpStream,
        addr: SocketAddr,
    },
    TcpServer {
        listener: TcpListener,
        clients: Arc<Mutex<Vec<TcpClientConn>>>,
    },
    Udp {
        read_sock: Arc<UdpSocket>,
    },
}

/*
*   描述：按类型建立 socket（不持任何锁；connect/bind 可能阻塞）
*   返回：(句柄, 待启动的读线程资源, 停止标志)
*/
fn build_connection(
    kind: NetKind,
    host: &str,
    port: u16,
    local_port: u16,
    local_host: Option<&str>,
) -> Result<(NetHandle, NetReadSetup, Arc<AtomicBool>), String> {
    match kind {
        NetKind::TcpClient => {
            let target = host.trim();
            if target.is_empty() {
                return Err("目标地址不能为空".into());
            }
            // 域名可能解析出多个地址（IPv4/IPv6）：逐个尝试，单个地址 5s 超时，避免一次失败就整体失败
            let addrs: Vec<SocketAddr> = (target, port)
                .to_socket_addrs()
                .map_err(|e| format!("地址解析失败: {}", e))?
                .collect();
            if addrs.is_empty() {
                return Err("地址解析失败".into());
            }
            let mut last_err = String::from("连接失败");
            let mut connected: Option<(TcpStream, SocketAddr)> = None;
            for addr in addrs {
                match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
                    Ok(stream) => {
                        connected = Some((stream, addr));
                        break;
                    }
                    Err(e) => last_err = format!("连接 {} 失败: {}", addr, e),
                }
            }
            let (stream, peer) = connected.ok_or(last_err)?;
            // 写超时防止慢消费者拖死写路径；读走阻塞模式，关闭时由 shutdown(Both) 唤醒
            stream
                .set_write_timeout(Some(Duration::from_millis(1000)))
                .map_err(|e| format!("设置写入超时失败: {}", e))?;
            // 禁用 Nagle：调试工具的小包必须立即发出，避免 200ms+ 的合并延迟
            stream
                .set_nodelay(true)
                .map_err(|e| format!("设置 TCP_NODELAY 失败: {}", e))?;
            let rd = stream
                .try_clone()
                .map_err(|e| format!("克隆读句柄失败: {}", e))?;
            let stop_flag = Arc::new(AtomicBool::new(false));
            let handle = NetHandle {
                kind,
                label: format!("TCP {}", peer),
                stop_flag: stop_flag.clone(),
                stream: Some(Arc::new(Mutex::new(stream))),
                clients: None,
                socket: None,
            };
            Ok((
                handle,
                NetReadSetup::TcpClient { rd, addr: peer },
                stop_flag,
            ))
        }
        NetKind::TcpServer => {
            let bind_host = if host.trim().is_empty() {
                "0.0.0.0"
            } else {
                host.trim()
            };
            // 元组绑定：主机名/IPv6 字面量都交给标准库解析，避免 "{}:{}" 拼接在 IPv6 下出错
            let listener = TcpListener::bind((bind_host, port))
                .map_err(|e| format!("监听 {}:{} 失败: {}", bind_host, port, e))?;
            listener
                .set_nonblocking(true)
                .map_err(|e| format!("设置非阻塞失败: {}", e))?;
            let label = match listener.local_addr() {
                Ok(addr) => format!("TCP服务 {}", addr),
                Err(_) => format!("TCP服务 {}:{}", bind_host, port),
            };
            let clients = Arc::new(Mutex::new(Vec::new()));
            let stop_flag = Arc::new(AtomicBool::new(false));
            let handle = NetHandle {
                kind,
                label,
                stop_flag: stop_flag.clone(),
                stream: None,
                clients: Some(clients.clone()),
                socket: None,
            };
            Ok((
                handle,
                NetReadSetup::TcpServer { listener, clients },
                stop_flag,
            ))
        }
        NetKind::Udp => {
            let bind_port = if local_port > 0 { local_port } else { port };
            let bind_host = local_host
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("0.0.0.0");
            let socket = UdpSocket::bind((bind_host, bind_port))
                .map_err(|e| format!("绑定 {}:{} 失败: {}", bind_host, bind_port, e))?;
            socket
                .set_read_timeout(Some(Duration::from_millis(POLL_INTERVAL_MS)))
                .map_err(|e| format!("设置读取超时失败: {}", e))?;

            // 目标地址非空则固定默认发送目标（connect 后只收发该对端，贴近调试工具用法）
            let target = host.trim();
            if !target.is_empty() && port > 0 {
                socket
                    .connect((target, port))
                    .map_err(|e| format!("设置目标 {}:{} 失败: {}", target, port, e))?;
            }
            let label = match socket.local_addr() {
                Ok(addr) => format!("UDP {}", addr),
                Err(_) => format!("UDP {}:{}", bind_host, bind_port),
            };
            // 读线程使用 try_clone 的独立套接字：recv_from 阻塞等待时不与写路径互相影响
            let read_sock = socket
                .try_clone()
                .map_err(|e| format!("克隆 UDP 套接字失败: {}", e))?;
            let stop_flag = Arc::new(AtomicBool::new(false));
            let handle = NetHandle {
                kind,
                label,
                stop_flag: stop_flag.clone(),
                stream: None,
                clients: None,
                socket: Some(Arc::new(socket)),
            };
            Ok((
                handle,
                NetReadSetup::Udp {
                    read_sock: Arc::new(read_sock),
                },
                stop_flag,
            ))
        }
    }
}

/*
*   描述：枚举本机所有网卡的 IPv4 地址（不含回环），供前端地址下拉快速选择
*   调用方法：前端调用: invoke("net_local_ips")
*   传参：无
*   返回: 本机 IPv4 地址列表（如 192.168.1.100）
*/
#[tauri::command]
pub async fn net_local_ips() -> Vec<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut ips: Vec<String> = if_addrs::get_if_addrs()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|iface| match iface.addr {
                if_addrs::IfAddr::V4(v4) => Some(v4.ip.to_string()),
                _ => None,
            })
            .filter(|ip| !ip.starts_with("127."))
            .collect();
        ips.sort();
        ips.dedup();
        ips
    })
    .await
    .unwrap_or_default()
}

/*
*   描述：打开网络连接（TCP 客户端 / TCP 服务端 / UDP）
*   调用方法：前端调用: invoke("net_open")
*   传参：key：会话唯一标识, kind：tcp_client/tcp_server/udp,
*         host：目标地址(服务端为监听地址, 可空默认 0.0.0.0),
*         port：目标端口(服务端为监听端口),
*         local_port：UDP 本地绑定端口(0 表示自动分配),
*         local_host：UDP 本地绑定地址(可空默认 0.0.0.0 监听所有网卡)
*   返回: 错误状态
*   说明：打开流程：加锁查重+占位 opening → 放锁建 socket → 加锁登记句柄 → 启动读线程/accept 线程，
*         并发打开同一 key 只有一次成功，且线程启动时句柄一定已在状态表中。
*/
#[tauri::command]
pub async fn net_open(
    key: String,
    kind: String,
    host: String,
    port: u16,
    local_port: u16,
    local_host: Option<String>,
    state: tauri::State<'_, SharedNetState>,
    app_handle: AppHandle,
) -> Result<String, String> {
    let kind = NetKind::parse(&kind).ok_or_else(|| format!("未知网络类型: {}", kind))?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // ① 加锁查重并占位：锁只用来登记，查完立刻放
        {
            let mut guard = state.lock().map_err(|e| e.to_string())?;
            if guard.conns.contains_key(&key) || guard.opening.contains(&key) {
                return Err(format!("连接已打开: {}", key));
            }
            guard.opening.insert(key.clone());
        }

        // ② 建立 socket（不持锁，卡多久都不影响其他命令）
        let built = build_connection(kind, &host, port, local_port, local_host.as_deref());
        let (handle, setup, stop_flag) = match built {
            Ok(v) => v,
            Err(e) => {
                // 失败必须清理占位，否则该会话会永久卡在“正在打开”
                if let Ok(mut guard) = state.lock() {
                    guard.opening.remove(&key);
                }
                return Err(e);
            }
        };
        let label = handle.label.clone();

        // ③ 先登记句柄，再启动读线程/accept 线程（线程启动时句柄必然可见）
        {
            let mut guard = state.lock().map_err(|e| e.to_string())?;
            guard.opening.remove(&key);
            guard.conns.insert(key.clone(), handle);
        }

        // ④ 启动后台线程
        match setup {
            NetReadSetup::TcpClient { rd, addr } => {
                tcp_read_thread(key, addr, rd, None, stop_flag, app_handle)
            }
            NetReadSetup::TcpServer { listener, clients } => {
                tcp_accept_thread(key, listener, clients, stop_flag, app_handle)
            }
            NetReadSetup::Udp { read_sock } => {
                udp_read_thread(key, read_sock, stop_flag, app_handle)
            }
        }

        Ok(format!("连接 {} 已打开", label))
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/*
*   描述：关闭指定网络连接
*   说明：锁内只摘除句柄并停自动发送，锁外 shutdown 套接字，让阻塞读线程立即退出；
*         TCP 服务端的每条已接入客户端也会逐个 shutdown，监听端口随 accept 线程退出释放
*   调用方法：前端调用: invoke("net_close")
*   传参：key：会话唯一标识
*   返回: 错误状态
*/
#[tauri::command]
pub async fn net_close(
    key: String,
    state: tauri::State<'_, SharedNetState>,
) -> Result<String, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (handle, auto_send) = {
            let mut guard = state.lock().map_err(|e| e.to_string())?;
            let handle = guard
                .conns
                .remove(&key)
                .ok_or(format!("连接未打开: {}", key))?;
            (handle, guard.auto_sends.remove(&key))
        };

        if let Some(task) = auto_send {
            task.store(true, Ordering::SeqCst);
        }
        let label = handle.label.clone();
        handle.shutdown();
        Ok(format!("连接 {} 已关闭", label))
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/*
*   描述：同步版全量关闭（供应用退出事件调用——退出路径不能走异步任务）
*/
pub fn close_all_sync(state: &SharedNetState) {
    let mut handles: Vec<NetHandle> = Vec::new();
    let mut auto_sends: Vec<Arc<AtomicBool>> = Vec::new();
    if let Ok(mut s) = state.lock() {
        auto_sends.extend(s.auto_sends.drain().map(|(_, task)| task));
        handles.extend(s.conns.drain().map(|(_, handle)| handle));
    }
    // 锁已释放：先停自动发送，再 shutdown 全部连接唤醒读线程
    for task in auto_sends {
        task.store(true, Ordering::SeqCst);
    }
    for handle in handles {
        handle.shutdown();
    }
}

/*
*   描述：关闭全部网络连接（前端页面重载/HMR 后会话列表清空，后端残留的监听/连接
*         若不释放会一直占用端口，导致"明明没使用却绑定失败 os error 10048"）
*   调用方法：前端初始化（initApp）时调用一次，保证后端状态与界面一致
*/
#[tauri::command]
pub async fn net_close_all(state: tauri::State<'_, SharedNetState>) -> Result<usize, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let count = state.lock().map_err(|e| e.to_string())?.conns.len();
        close_all_sync(&state);
        Ok(count)
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/*
*   描述：通过网络连接发送数据。TCP 服务端会向所有已接入客户端广播。
*   调用方法：前端调用: invoke("net_write")
*   传参：key：会话唯一标识, data：待发送文本, bytes：二进制原始字节（优先于 data）
*   返回: 错误状态
*/
#[tauri::command]
pub async fn net_write(
    key: String,
    data: String,
    bytes: Option<Vec<u8>>,
    state: tauri::State<'_, SharedNetState>,
) -> Result<String, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // 锁内只 clone 写目标，实际写操作在锁外进行
        let targets = {
            let guard = state.lock().map_err(|e| e.to_string())?;
            let handle = guard
                .conns
                .get(&key)
                .ok_or(format!("连接未打开: {}", key))?;
            handle.write_targets()
        };
        // 二进制模式优先使用原始字节（十六进制发送），否则按 UTF-8 字符串
        let payload = bytes.unwrap_or_else(|| data.as_bytes().to_vec());
        let sent = net_write_bytes(&targets, &payload)?;
        Ok(match targets.kind {
            NetKind::TcpServer => {
                format!("已向 {} 个客户端发送 {} 字节", sent, payload.len())
            }
            _ => format!("已发送 {} 字节", payload.len()),
        })
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/*
*   描述：启动自动发送（Rust 线程按间隔直接写网络连接，绕过前端 IPC）
*   传参：key：会话唯一标识, data：待发送文本, bytes：二进制原始字节（优先于 data）,
*         interval_ms：间隔（1ms ~ 1 小时）
*/
#[tauri::command]
pub async fn net_auto_send_start(
    key: String,
    data: String,
    bytes: Option<Vec<u8>>,
    interval_ms: u64,
    state: tauri::State<'_, SharedNetState>,
    app_handle: AppHandle,
) -> Result<String, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // 二进制模式优先使用原始字节，与 net_write 保持一致
        let payload = bytes.unwrap_or_else(|| data.into_bytes());
        let interval_ms = interval_ms.clamp(1, 3_600_000);
        let stop = Arc::new(AtomicBool::new(false));

        // 锁内只登记停止标志并确认连接存在，写操作在每轮循环里取到目标快照后放锁执行
        let label = {
            let mut guard = state.lock().map_err(|e| e.to_string())?;
            // 已有任务先停
            if let Some(old) = guard.auto_sends.remove(&key) {
                old.store(true, Ordering::SeqCst);
            }
            let handle = guard
                .conns
                .get(&key)
                .ok_or(format!("连接未打开: {}", key))?;
            let label = handle.label.clone();
            guard.auto_sends.insert(key.clone(), stop.clone());
            label
        };

        let interval = Duration::from_millis(interval_ms);
        thread::spawn(move || loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            crate::serial::wait_precisely(interval);
            if stop.load(Ordering::Relaxed) {
                break;
            }

            // 锁内只 clone 写目标，驱动写放到锁外
            let targets = match state.lock() {
                Ok(guard) => guard.conns.get(&key).map(|handle| handle.write_targets()),
                Err(_) => None,
            };
            let Some(targets) = targets else {
                break;
            };

            match net_write_bytes(&targets, &payload) {
                // 写失败或没有可用客户端：通知前端弹回自动发送开关，并退出线程
                Ok(sent) if sent > 0 => {
                    // 通知前端记录 TX
                    let _ = app_handle.emit(
                        "auto-sent",
                        AutoSentEvent {
                            key: key.clone(),
                            data: payload.clone(),
                        },
                    );
                }
                Ok(_) => {
                    let _ = app_handle.emit(
                        "auto-send-stopped",
                        AutoSentEvent {
                            key: key.clone(),
                            data: "没有可发送的客户端".as_bytes().to_vec(),
                        },
                    );
                    break;
                }
                Err(e) => {
                    let _ = app_handle.emit(
                        "auto-send-stopped",
                        AutoSentEvent {
                            key: key.clone(),
                            data: e.into_bytes(),
                        },
                    );
                    break;
                }
            }
        });
        Ok(format!("{} 自动发送已启动，间隔 {}ms", label, interval_ms))
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}

/*
*   描述：停止指定会话的自动发送任务
*/
#[tauri::command]
pub async fn net_auto_send_stop(
    key: String,
    state: tauri::State<'_, SharedNetState>,
) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = state.lock().map_err(|e| e.to_string())?;
        if let Some(task) = guard.auto_sends.remove(&key) {
            task.store(true, Ordering::SeqCst);
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}
