// nn —— NNSerialTool 命令行配套工具：给 AI / 脚本直接读串口与网络设备日志。
//
// 设计要点：
//   - 零 Tauri 依赖的独立小二进制（不复用 GUI 的 lib，避免拖入整个 Tauri 运行时）
//   - 数据走 stdout（行缓冲、无 ANSI 花哨字符），状态/错误走 stderr——AI 只管读 stdout
//   - 退出码规范：0 成功 / 2 参数错误 / 3 端口或连接打开失败
//   - Windows 串口独占：GUI 占用同一端口时本工具打不开（报错提示），反之亦然
//   - 日志落盘：默认写入 exe 同级 NNlog/ 目录（自动创建），--out 可指定文件路径；
//     写入内容与 stdout 完全一致（解码后的行），便于事后 tail/检索
//
// 命令：
//   nn ports                                   列出串口
//   nn log  -p COM3 -b 115200 [--hex] [--nnwave]
//          [--lines N] [--duration S] [--out F] 实时读串口输出
//   nn send -p COM3 -b 115200 -d "AT\r\n" [--hex]
//                                              发送
//   nn tcp  --host H --port P [同 log 过滤参数]  读 TCP 设备日志
//   nn udp  --port P [同 log 过滤参数]           收 UDP 日志
//   nn version [--json]                        打印 nn / 应用版本与协议版本
//   nn check-update [--json]                   检查 GitHub Release 是否有新版本
//   nn install-skill                           安装 AI SKILL 文件
//
// --nnwave：按 NN-Wave 协议解码输出 CSV（t_ms,ch1,ch2,...）——解码为 nn 内置实现，
// 与波形视图 / 固件模板 nnwave.c 同协议（AA 55 | type | N | seq | f32×N | CRC8），
// 无需安装任何波形插件。
use serialport::SerialPortType;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs, UdpSocket};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const EXIT_OK: i32 = 0;
const EXIT_ARGS: i32 = 2;
const EXIT_OPEN: i32 = 3;
// 检查更新专用退出码：4=检查失败（网络/解析），10=发现新版本（便于脚本判断）
const EXIT_NET: i32 = 4;
const EXIT_UPDATE: i32 = 10;
const MAX_CHANNELS: usize = 64;
// 更新源与主程序保持一致：GitHub Release 优先，jsDelivr 备源
const UPDATE_URLS: [&str; 2] = [
    "https://github.com/lsx666xsl/NNSerialTool/releases/latest/download/latest.json",
    "https://cdn.jsdelivr.net/gh/lsx666xsl/NNSerialTool@master/latest.json",
];
const REPO_URL: &str = "https://github.com/lsx666xsl/NNSerialTool";

fn print_usage() {
    println!(
        "nn —— NNSerialTool 命令行工具（供 AI / 脚本直接读取设备日志）

用法:
  nn ports                                列出串口
  nn log  -p <口> [-b <波特率>] [--hex] [--nnwave]
          [--lines N] [--duration S] [--out <文件>]
                                          实时读串口输出到 stdout
  nn send -p <口> [-b <波特率>] -d <内容> [--hex]
                                          发送文本或 HEX 字节
  nn tcp  --host <地址> --port <端口> [同 log 过滤参数]
                                          读 TCP 设备日志
  nn udp  --port <本地端口> [同 log 过滤参数]
                                          收 UDP 日志
  nn version [--json]                     打印版本（nn / 应用 / NN-Wave 协议）
  nn check-update [--json]                检查是否有新版本（0=最新，10=有更新，4=检查失败）
  nn examples                             查看常用命令示例（串口/TCP/UDP/HEX/自动采集）
  nn help [子命令]                        查看某个子命令的完整参数与示例
  nn install-skill                        重装 AI SKILL 到 ~/.agents/skills/nn/（安装包缺失时用）

参数标记: <必选> 为必须填写，[可选] 可省略；例如 nn log -p COM3 -n 200 中 -p 必填、-n 可选。

参数:
  -p / --port <口>       串口名（log/send 用：Windows COM3 / Linux /dev/ttyUSB0 /
                         macOS /dev/tty.usbmodem*）；tcp/udp 子命令下为网络端口
  -b / --baud <速率>     波特率（默认 115200）
  -n / --lines <N>       读满 N 行后退出
  -t / --duration <秒>   持续 N 秒后退出（可与 --lines 组合，先到先退）
  -H / --host <地址>     tcp 目标地址
  --port-tcp <端口>      tcp/udp 网络端口的显式写法（与 --port 等价）
  -x / --hex             以十六进制输出原始字节；send 时 -d 按 HEX 解析
  -w / --nnwave          按 NN-Wave 协议解码为 CSV: t_ms,ch1,ch2,...
                         （解码为 nn 内置，无需安装波形插件）
  -d / --data <内容>     发送内容（--hex 时为 HEX 字符串）
  -o / --out <路径>      日志落盘路径：给目录则自动生成 <端口>-<时间戳>.log；
                         给 .log 等文件路径则原样写入；目录不存在会自动创建
                         （默认 exe 同级 NNlog/<来源>-<时间戳>.log）
  --json                 仅 version / check-update 使用：输出机器可读 JSON

退出码: 0 成功 / 2 参数错误 / 3 打开失败 / 4 检查更新失败 / 10 有新版本"
    );
}

struct Opts {
    port: String,
    baud: u32,
    hex: bool,
    nnwave: bool,
    lines: Option<usize>,
    duration: Option<u64>,
    host: String,
    tcp_port: u16,
    data: String,
    out: Option<String>,
}

fn parse_opts(args: &[String]) -> Result<Opts, String> {
    let mut o = Opts {
        port: String::new(),
        baud: 115_200,
        hex: false,
        nnwave: false,
        lines: None,
        duration: None,
        host: String::new(),
        tcp_port: 0,
        data: String::new(),
        out: None,
    };
    let mut i = 0;
    while i < args.len() {
        let take_value = |i: &mut usize, name: &str| -> Result<String, String> {
            *i += 1;
            args.get(*i)
                .cloned()
                .ok_or_else(|| format!("参数 {} 缺少值", name))
        };
        match args[i].as_str() {
            "-p" | "--port" => o.port = take_value(&mut i, "-p")?,
            "-b" | "--baud" => {
                let v = take_value(&mut i, "-b")?;
                o.baud = v.parse().map_err(|_| format!("波特率无效: {}", v))?;
            }
            "-n" | "--lines" => {
                let v = take_value(&mut i, "--lines")?;
                o.lines = Some(v.parse().map_err(|_| format!("行数无效: {}", v))?);
            }
            "-t" | "--duration" => {
                let v = take_value(&mut i, "--duration")?;
                o.duration = Some(v.parse().map_err(|_| format!("时长无效: {}", v))?);
            }
            "-H" | "--host" => o.host = take_value(&mut i, "-H")?,
            "--port-tcp" => {
                let v = take_value(&mut i, "--port-tcp")?;
                o.tcp_port = v.parse().map_err(|_| format!("端口无效: {}", v))?;
            }
            "-d" | "--data" => o.data = take_value(&mut i, "-d")?,
            "-o" | "--out" => o.out = Some(take_value(&mut i, "-o")?),
            "-x" | "--hex" => o.hex = true,
            "-w" | "--nnwave" => o.nnwave = true,
            other => return Err(format!("未知参数: {}", other)),
        }
        i += 1;
    }
    Ok(o)
}

// ---------- 输出端：stdout + 可选日志文件双写 ----------
enum SinkKind {
    Text {
        line: Vec<u8>,
    },
    Hex,
    NnWave {
        buf: Vec<u8>,
        seq_last: i32,
        t0: Instant,
    },
}

struct Sink {
    kind: SinkKind,
    log: Option<File>,
    // 日志写入失败只提示一次，避免刷屏（但绝不静默吞掉首次错误）
    log_warned: bool,
}

impl Sink {
    fn new(o: &Opts, log_path: &PathBuf, t0: Instant) -> Sink {
        let log = match OpenOptions::new().create(true).append(true).open(log_path) {
            Ok(f) => Some(f),
            Err(e) => {
                eprintln!(
                    "[nn] 警告: 无法写入日志文件 {}（{}），本次仅输出到 stdout",
                    log_path.display(),
                    e
                );
                None
            }
        };
        let kind = if o.nnwave {
            SinkKind::NnWave {
                buf: Vec::new(),
                seq_last: -1,
                t0,
            }
        } else if o.hex {
            SinkKind::Hex
        } else {
            SinkKind::Text { line: Vec::new() }
        };
        Sink {
            kind,
            log,
            log_warned: false,
        }
    }

    // 喂一段原始字节；返回输出的行数（供 --lines 退出条件使用）
    fn feed(&mut self, data: &[u8]) -> usize {
        let mut emitted = 0usize;
        // 单行上限 64KB：设备输出二进制/无换行流时防止内存无界增长
        const MAX_LINE: usize = 64 * 1024;
        let texts: Vec<String> = match &mut self.kind {
            SinkKind::Text { line } => {
                let mut out = Vec::new();
                for &b in data {
                    if b == b'\n' {
                        out.push(
                            String::from_utf8_lossy(line)
                                .trim_end_matches('\r')
                                .to_string(),
                        );
                        line.clear();
                        emitted += 1;
                    } else {
                        // 按字节累积，整行后再做 UTF-8 解码：多字节字符跨读块也不会乱码
                        line.push(b);
                        if line.len() >= MAX_LINE {
                            eprintln!("[nn] 警告: 单行超过 {}KB，已强制切行输出", MAX_LINE / 1024);
                            out.push(String::from_utf8_lossy(line).to_string());
                            line.clear();
                            emitted += 1;
                        }
                    }
                }
                out
            }
            SinkKind::Hex => {
                if data.is_empty() {
                    Vec::new()
                } else {
                    emitted = 1;
                    vec![data
                        .iter()
                        .map(|b| format!("{:02X}", b))
                        .collect::<Vec<_>>()
                        .join(" ")]
                }
            }
            SinkKind::NnWave {
                buf,
                seq_last,
                t0: start,
            } => {
                buf.extend_from_slice(data);
                let mut out = Vec::new();
                // 与固件/上位机一致的状态机：扫帧头 → 校验长度与 CRC8 → 出帧
                loop {
                    if buf.len() < 6 {
                        break;
                    }
                    if buf[0] != 0xAA || buf[1] != 0x55 {
                        buf.remove(0);
                        continue;
                    }
                    let n = buf[3] as usize;
                    if n == 0 || n > MAX_CHANNELS {
                        buf.remove(0);
                        continue;
                    }
                    let frame_len = 6 + 4 * n;
                    if buf.len() < frame_len {
                        break;
                    }
                    let crc = nnwave_crc8(&buf[2..frame_len - 1]);
                    if crc != buf[frame_len - 1] {
                        buf.remove(0);
                        continue;
                    }
                    if buf[2] == 0x01 {
                        let mut vals = Vec::with_capacity(n);
                        for c in 0..n {
                            let v = f32::from_le_bytes([
                                buf[5 + c * 4],
                                buf[6 + c * 4],
                                buf[7 + c * 4],
                                buf[8 + c * 4],
                            ]);
                            vals.push(format!("{:.6}", v));
                        }
                        let dt = start.elapsed().as_millis();
                        out.push(format!("{},{}", dt, vals.join(",")));
                        emitted += 1;
                    }
                    // 元数据帧（0x02 通道名）CLI 不展示；seq 丢帧统计走 stderr
                    let seq = buf[4] as i32;
                    if *seq_last >= 0 {
                        let gap = (seq - *seq_last - 1) & 0xff;
                        if gap > 0 {
                            eprintln!("[nn] 丢帧 {} 帧", gap);
                        }
                    }
                    *seq_last = seq;
                    buf.drain(..frame_len);
                }
                out
            }
        };
        for text in &texts {
            println!("{}", text);
            if let Some(f) = &mut self.log {
                if let Err(e) = writeln!(f, "{}", text) {
                    if !self.log_warned {
                        self.log_warned = true;
                        eprintln!("[nn] 警告: 日志写入失败（{}），后续仅输出到 stdout", e);
                    }
                }
            }
        }
        io::stdout().flush().ok();
        if let Some(f) = &mut self.log {
            if let Err(e) = f.flush() {
                if !self.log_warned {
                    self.log_warned = true;
                    eprintln!("[nn] 警告: 日志刷新失败（{}）", e);
                }
            }
        }
        emitted
    }
}

fn nnwave_crc8(data: &[u8]) -> u8 {
    let mut crc = 0u8;
    for &b in data {
        crc ^= b;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x07
            } else {
                crc << 1
            };
        }
    }
    crc
}

// ---------- 日志文件路径：--out 优先，否则 exe 同级 NNlog/<端口>-<时间戳>.log ----------
// --out 的两种语义：
//   1) 目录（已存在的目录 / 以路径分隔符结尾 / 不存在且没有扩展名）：
//      在目录内自动生成 <来源>-<时间戳>.log，目录不存在会自动创建；
//   2) 文件（带扩展名或已存在的文件）：原样写入该文件，父目录不存在会自动创建。
fn resolve_out_path(raw: &str, generated_name: &str) -> Result<PathBuf, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("--out 不能为空".into());
    }
    let path = PathBuf::from(raw);
    let trailing_sep = raw.ends_with('/')
        || raw.ends_with('\\')
        || raw.ends_with(std::path::MAIN_SEPARATOR);
    // 无扩展名且不存在时按目录处理：用户给 D:\logs\new 这类路径时直接创建目录，
    // 往里面写自动命名的文件；要精确文件路径请写 D:\logs\new.log。
    let dir_intent = path.is_dir() || trailing_sep || (!path.exists() && path.extension().is_none());
    if dir_intent {
        fs::create_dir_all(&path).map_err(|e| format!("创建日志目录失败: {}", e))?;
        return Ok(path.join(generated_name));
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| format!("创建日志目录失败: {}", e))?;
        }
    }
    Ok(path)
}

fn resolve_log_path(o: &Opts, source: &str) -> Result<PathBuf, String> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    let safe: String = source
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let name = format!("{}-{}.log", safe, ts);
    if let Some(p) = &o.out {
        return resolve_out_path(p, &name);
    }
    // 首选 exe 同级 NNlog/；不可写（AppImage 只读挂载 / Linux 系统目录无权限）则回退 ~/NNlog/
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let dir = parent.join("NNlog");
            if fs::create_dir_all(&dir).is_ok() {
                let probe = dir.join(format!(".nnwrite-{}", ts));
                if fs::write(&probe, b"").is_ok() {
                    let _ = fs::remove_file(&probe);
                    return Ok(dir.join(name));
                }
            }
        }
    }
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".into());
    let dir = PathBuf::from(home).join("NNlog");
    fs::create_dir_all(&dir).map_err(|e| format!("创建日志目录失败: {}", e))?;
    Ok(dir.join(name))
}

// ---------- 版本与更新检查 ----------
const NN_VERSION: &str = env!("CARGO_PKG_VERSION");
const NN_PROTOCOL: &str = "NN-Wave v1";

// 语义化版本比较：只取前 3 段数字，忽略 -pre/+build 后缀（与主程序更新检测口径一致）
fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let core = v.trim().trim_start_matches('v');
    let core = core.split(['-', '+']).next().unwrap_or(core);
    let mut it = core.split('.');
    let major = it.next()?.parse().ok()?;
    let minor = it.next().unwrap_or("0").parse().unwrap_or(0);
    let patch = it.next().unwrap_or("0").parse().unwrap_or(0);
    Some((major, minor, patch))
}

fn version_gt(a: &str, b: &str) -> bool {
    match (parse_version(a), parse_version(b)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

// nn version：打印 nn / 应用 / 协议版本（--json 供脚本消费）
fn cmd_version(args: &[String]) -> i32 {
    let json = args.iter().any(|a| a == "--json");
    if json {
        println!(
            "{}",
            serde_json::json!({
                "name": "nn",
                "version": NN_VERSION,
                "app": "NNSerialTool",
                "appVersion": NN_VERSION,
                "protocol": NN_PROTOCOL,
                "repo": REPO_URL
            })
        );
    } else {
        println!("nn {}", NN_VERSION);
        println!("NNSerialTool {}", NN_VERSION);
        println!("协议: {}", NN_PROTOCOL);
    }
    EXIT_OK
}

// 拉取 latest.json（GitHub Release 优先，jsDelivr 备源）
fn fetch_latest_json() -> Result<(String, serde_json::Value), String> {
    // reqwest 使用 rustls-no-provider：必须安装进程级 CryptoProvider。
    // 主程序由 updater 插件负责安装；nn 是独立二进制，需要自己装（重复安装返回 Err 可忽略）。
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent(format!("nn/{}", NN_VERSION))
        .build()
        .map_err(|e| format!("初始化 HTTP 客户端失败: {}", e))?;
    let mut last_err = String::new();
    for url in UPDATE_URLS {
        match client.get(url).send() {
            Ok(resp) if resp.status().is_success() => match resp.text() {
                Ok(text) => match serde_json::from_str::<serde_json::Value>(&text) {
                    Ok(v) => return Ok((url.to_string(), v)),
                    Err(e) => last_err = format!("{} 返回的 latest.json 解析失败: {}", url, e),
                },
                Err(e) => last_err = format!("{} 读取响应失败: {}", url, e),
            },
            Ok(resp) => last_err = format!("{} 返回 HTTP {}", url, resp.status()),
            Err(e) => last_err = format!("{} 请求失败: {}", url, e),
        }
    }
    Err(last_err)
}

// nn check-update：检查 GitHub Release 是否有新版本
// 退出码：0 已是最新；10 有更新；4 检查失败（网络/解析）；2 参数错误
fn cmd_check_update(args: &[String]) -> i32 {
    let json = args.iter().any(|a| a == "--json");
    let (source, body) = match fetch_latest_json() {
        Ok(v) => v,
        Err(e) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "current": NN_VERSION,
                        "error": e,
                        "updateAvailable": false
                    })
                );
            } else {
                eprintln!("检查更新失败: {}", e);
            }
            return EXIT_NET;
        }
    };
    let latest = body
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let notes = body
        .get("notes")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if latest.is_empty() {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "current": NN_VERSION,
                    "error": "latest.json 缺少 version 字段",
                    "updateAvailable": false
                })
            );
        } else {
            eprintln!("检查更新失败: latest.json 缺少 version 字段");
        }
        return EXIT_NET;
    }
    let has_update = version_gt(&latest, NN_VERSION);
    if json {
        println!(
            "{}",
            serde_json::json!({
                "current": NN_VERSION,
                "latest": latest,
                "updateAvailable": has_update,
                "notes": notes,
                "source": source,
                "repo": REPO_URL
            })
        );
    } else if has_update {
        println!("当前版本: {}", NN_VERSION);
        println!("最新版本: {}", latest);
        println!("发现新版本，可前往下载: {}/releases/latest", REPO_URL);
        if !notes.trim().is_empty() {
            println!();
            println!("更新说明:");
            for line in notes.lines().take(20) {
                println!("{}", line);
            }
        }
    } else {
        // 本地版本可能已高于线上（开发/自编译场景）：把两个版本都打出来，避免误以为线上已是 0.6.4
        println!("当前已是最新版本（本地 v{}，线上 v{}）", NN_VERSION, latest);
    }
    if has_update {
        EXIT_UPDATE
    } else {
        EXIT_OK
    }
}

// ---------- 公共读取循环：限制条件（行数/时长）+ Ctrl+C 原生终止 ----------
fn limits_reached(o: &Opts, lines: usize, started: Instant) -> bool {
    if let Some(n) = o.lines {
        if lines >= n {
            return true;
        }
    }
    if let Some(s) = o.duration {
        if started.elapsed().as_secs_f64() >= s as f64 {
            return true;
        }
    }
    false
}

// ---------- 子命令 ----------
// 安装 AI SKILL 文件到 ~/.agents/skills/nn/SKILL.md（覆盖式）
// AI SKILL 内容与安装包资源同源：构建时直接内嵌 resources/nn/SKILL.md，
// 安装器也复制同一文件，避免两处文案漂移（以前 SKILL 内嵌在代码里，安装器另行调用 nn.exe 生成）。
const SKILL_MD: &str = include_str!("../../resources/nn/SKILL.md");

fn cmd_install_skill() -> i32 {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    if home.is_empty() {
        eprintln!("[nn] 无法定位用户目录（USERPROFILE/HOME 均未设置）");
        return EXIT_OPEN;
    }
    let dir = std::path::Path::new(&home)
        .join(".agents")
        .join("skills")
        .join("nn");
    if let Err(e) = fs::create_dir_all(&dir) {
        eprintln!("[nn] 创建目录失败: {}", e);
        return EXIT_OPEN;
    }
    let path = dir.join("SKILL.md");
    match fs::write(&path, SKILL_MD) {
        Ok(_) => {
            println!("SKILL 已安装: {}", path.display());
            EXIT_OK
        }
        Err(e) => {
            eprintln!("[nn] 写入失败: {}", e);
            EXIT_OPEN
        }
    }
}

// 分命令详细帮助：nn help [log|send|tcp|udp|ports|examples|install-skill]
fn cmd_examples() -> i32 {
    println!(
        "nn 常用命令示例
================
说明: <必选> 为必须填写，[可选] 可省略；完整参数用 nn help <子命令> 查看。

串口:
  nn ports                                     列出串口（含 USB vid/pid）
  nn log -p COM3 -b 115200 -n 200               采集 200 行文本后退出
  nn log -p COM3 -b 115200 -x -n 20             采集 20 行 HEX
  nn log -p COM3 -b 115200 -w -t 10             采集 10 秒并解码 NN-Wave 为 CSV
  nn log -p COM3 -b 115200 -o D:\\logs\\         目录：自动生成 COM3-<时间戳>.log
  nn log -p COM3 -b 115200 -o D:\\logs\\dev.log  文件：原样写入指定文件
  nn send -p COM3 -b 115200 -d \"AT\\r\\n\"       发送文本（换行符按需写）
  nn send -p COM3 -b 115200 -x -d \"01 03 00 00\" 发送 HEX 字节

网络:
  nn tcp -H 192.168.1.10 --port 9000 -w -n 100  读 TCP 设备（NN-Wave 解码）
  nn udp --port 9000 -w -n 50                   收 UDP 数据报

其他:
  nn version --json                             版本信息（JSON）
  nn check-update                              检查更新（0=最新，10=有更新，4=失败）
  nn install-skill                             重装 AI SKILL 文件

提示:
  - 串口被 GUI 占用时 nn 会打开失败：先在 NNSerialTool 里断开该端口
  - 数据只写 stdout；状态/丢帧统计在 stderr，重定向时只取 stdout 即为纯数据"
    );
    EXIT_OK
}

fn cmd_help(topic: Option<&str>) -> i32 {
    match topic {
        Some("log") => println!(
            "nn log —— 实时读取串口输出（数据走 stdout，状态走 stderr）

用法:
  nn log -p COM3 [-b 115200] [过滤/格式参数]

标记: <必选> 为必须填写，[可选] 可省略。

参数:
  -p <口>        串口名
  -b <速率>      波特率（默认 115200）
  -n <N>         读满 N 行退出
  -t <秒>        采集 N 秒退出
  -w             NN-Wave 解码为 CSV: t_ms,ch1,ch2,...（内置解码，无需波形插件）
  -x             十六进制输出原始字节
  -o <路径>      日志落盘路径：目录（不存在会自动创建）内自动生成
                 <端口>-<时间戳>.log；带扩展名的文件路径则原样写入

示例:
  nn log -p COM3 -w -t 10        采集 10 秒并解码为 CSV
  nn log -p COM3 -n 200          读 200 行日志"
        ),
        Some("send") => println!(
            "nn send —— 发送文本或 HEX 字节

用法:
  nn send -p COM3 [-b 115200] -d <内容> [-x]

标记: <必选> 为必须填写，[可选] 可省略。

示例:
  nn send -p COM3 -d \"AT\\r\\n\"
  nn send -p COM3 -x -d \"01 03 00 00 00 02\""
        ),
        Some("tcp") => println!(
            "nn tcp —— 读取 TCP 设备日志

用法:
  nn tcp -H <地址> --port <端口> [-w] [-x] [-n N] [-t 秒] [-o 文件]

标记: <必选> 为必须填写，[可选] 可省略。

示例:
  nn tcp -H 127.0.0.1 --port 9000 -w -n 100"
        ),
        Some("udp") => println!(
            "nn udp —— 接收 UDP 数据报

用法:
  nn udp --port <本地端口> [-w] [-x] [-n N] [-t 秒]

标记: <必选> 为必须填写，[可选] 可省略。"
        ),
        Some("examples") | Some("example") => return cmd_examples(),
        Some("install-skill") => println!(
            "nn install-skill —— 把 AI SKILL 写入用户目录（覆盖式）

用法:
  nn install-skill

目标路径: %USERPROFILE%\\.agents\\skills\\nn\\SKILL.md（Windows）；
          $HOME/.agents/skills/nn/SKILL.md（Linux/macOS）
退出码: 0 成功 / 3 创建目录或写文件失败"
        ),
        Some("ports") | Some("version") | Some("check-update") | None => print_usage(),
        Some(other) => {
            println!(
                "未知主题: {}（可选 log/send/tcp/udp/ports/version/check-update/examples/install-skill）",
                other
            );
            return EXIT_ARGS;
        }
    }
    EXIT_OK
}

fn cmd_ports() -> i32 {
    match serialport::available_ports() {
        Ok(list) => {
            for p in list {
                match p.port_type {
                    SerialPortType::UsbPort(info) => {
                        println!(
                            "{}  USB vid={:04x} pid={:04x} {} {}",
                            p.port_name,
                            info.vid,
                            info.pid,
                            info.product.as_deref().unwrap_or(""),
                            info.serial_number.as_deref().unwrap_or("")
                        );
                    }
                    other => println!("{}  {:?}", p.port_name, other),
                }
            }
            EXIT_OK
        }
        Err(e) => {
            eprintln!("枚举串口失败: {}", e);
            EXIT_OPEN
        }
    }
}

fn open_serial(o: &Opts) -> Result<Box<dyn serialport::SerialPort>, (String, i32)> {
    if o.port.is_empty() {
        return Err(("缺少串口名（-p COM3）".into(), EXIT_ARGS));
    }
    serialport::new(&o.port, o.baud)
        .timeout(Duration::from_millis(100))
        .open()
        .map_err(|e| {
            (
                format!(
                    "打开 {} 失败: {}（若被 GUI 占用请先在界面关闭连接）",
                    o.port, e
                ),
                EXIT_OPEN,
            )
        })
}

fn cmd_log(args: &[String]) -> i32 {
    let o = match parse_opts(args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("参数错误: {}", e);
            return EXIT_ARGS;
        }
    };
    let log_path = match resolve_log_path(&o, &o.port) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{}", e);
            return EXIT_OPEN;
        }
    };
    let mut port = match open_serial(&o) {
        Ok(p) => p,
        Err((msg, code)) => {
            eprintln!("{}", msg);
            return code;
        }
    };
    eprintln!("[nn] {} @ {} 已打开（Ctrl+C 退出）", o.port, o.baud);
    if o.nnwave {
        eprintln!("[nn] --nnwave 为内置协议解码，无需安装波形插件");
    }
    eprintln!("[nn] 日志文件: {}", log_path.display());
    let started = Instant::now();
    let mut sink = Sink::new(&o, &log_path, started);
    let mut lines = 0usize;
    let mut buf = [0u8; 4096];
    loop {
        if limits_reached(&o, lines, started) {
            break;
        }
        match port.read(&mut buf) {
            Ok(0) => std::thread::sleep(Duration::from_millis(10)), // 异常设备的空读，避免忙等
            Ok(n) => {
                lines += sink.feed(&buf[..n]);
            }
            Err(ref e)
                if e.kind() == io::ErrorKind::TimedOut || e.kind() == io::ErrorKind::WouldBlock => {
            }
            Err(e) => {
                eprintln!("[nn] 读取错误: {}", e);
                return EXIT_OPEN;
            }
        }
    }
    EXIT_OK
}

// 把 -d 内容转换为待发送字节：文本按 UTF-8；--hex 严格解析（偶数长度纯十六进制）。
// 抽成纯函数便于单测；任何非法输入返回 Err（调用方给 EXIT_ARGS），绝不静默丢字节。
fn parse_output_payload(data: &str, hex: bool) -> Result<Vec<u8>, String> {
    if !hex {
        return Ok(data.as_bytes().to_vec());
    }
    let cleaned: String = data.chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.is_empty()
        || cleaned.len() % 2 != 0
        || !cleaned.chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err("--hex 内容必须是偶数长度的十六进制字符串（如 \"01 03 00 00\"）".into());
    }
    Ok((0..cleaned.len() / 2)
        .filter_map(|i| u8::from_str_radix(&cleaned[i * 2..i * 2 + 2], 16).ok())
        .collect())
}

fn cmd_send(args: &[String]) -> i32 {
    let o = match parse_opts(args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("参数错误: {}", e);
            return EXIT_ARGS;
        }
    };
    if o.data.is_empty() {
        eprintln!("参数错误: 缺少发送内容（-d）");
        return EXIT_ARGS;
    }
    // 先做纯参数校验再打开串口：非法 HEX 不应占用/试探设备端口
    let payload = match parse_output_payload(&o.data, o.hex) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("参数错误: {}", e);
            return EXIT_ARGS;
        }
    };
    let mut port = match open_serial(&o) {
        Ok(p) => p,
        Err((msg, code)) => {
            eprintln!("{}", msg);
            return code;
        }
    };
    match port.write_all(&payload).and_then(|_| port.flush()) {
        Ok(_) => {
            println!("已发送 {} 字节 → {}", payload.len(), o.port);
            EXIT_OK
        }
        Err(e) => {
            eprintln!("发送失败: {}", e);
            EXIT_OPEN
        }
    }
}

fn cmd_tcp(args: &[String]) -> i32 {
    let mut o = match parse_opts(args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("参数错误: {}", e);
            return EXIT_ARGS;
        }
    };
    // 端口参数归一：--port / --port-tcp 均可
    if o.tcp_port == 0 && !o.port.is_empty() {
        o.tcp_port = o.port.parse().unwrap_or(0);
    }
    if o.host.is_empty() {
        o.host = "127.0.0.1".into(); // 本地设备是最常见场景，缺省回环地址
    }
    if o.tcp_port == 0 {
        eprintln!("参数错误: 需要 --port");
        return EXIT_ARGS;
    }
    let log_path = match resolve_log_path(&o, &format!("tcp-{}", o.tcp_port)) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{}", e);
            return EXIT_OPEN;
        }
    };
    let mut stream = match TcpStream::connect_timeout(
        &match (o.host.as_str(), o.tcp_port)
            .to_socket_addrs()
            .ok()
            .and_then(|mut it| it.next())
        {
            Some(a) => a,
            None => {
                eprintln!("地址解析失败: {}:{}", o.host, o.tcp_port);
                return EXIT_OPEN;
            }
        },
        Duration::from_secs(5),
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("连接 {}:{} 失败: {}", o.host, o.tcp_port, e);
            return EXIT_OPEN;
        }
    };
    eprintln!("[nn] {}:{} 已连接（Ctrl+C 退出）", o.host, o.tcp_port);
    eprintln!("[nn] 日志文件: {}", log_path.display());
    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .ok();
    let started = Instant::now();
    let mut sink = Sink::new(&o, &log_path, started);
    let mut lines = 0usize;
    let mut buf = [0u8; 4096];
    loop {
        if limits_reached(&o, lines, started) {
            break;
        }
        match stream.read(&mut buf) {
            Ok(0) => {
                eprintln!("[nn] 对端关闭连接");
                break;
            }
            Ok(n) => {
                lines += sink.feed(&buf[..n]);
            }
            Err(ref e)
                if e.kind() == io::ErrorKind::TimedOut || e.kind() == io::ErrorKind::WouldBlock => {
            }
            Err(e) => {
                eprintln!("[nn] 读取错误: {}", e);
                return EXIT_OPEN;
            }
        }
    }
    EXIT_OK
}

fn cmd_udp(args: &[String]) -> i32 {
    let mut o = match parse_opts(args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("参数错误: {}", e);
            return EXIT_ARGS;
        }
    };
    // 端口参数归一：--port / --port-tcp 均可
    if o.tcp_port == 0 && !o.port.is_empty() {
        o.tcp_port = o.port.parse().unwrap_or(0);
    }
    if o.tcp_port == 0 {
        eprintln!("参数错误: 需要本地端口（--port）");
        return EXIT_ARGS;
    }
    let log_path = match resolve_log_path(&o, &format!("udp-{}", o.tcp_port)) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{}", e);
            return EXIT_OPEN;
        }
    };
    let sock = match UdpSocket::bind(("0.0.0.0", o.tcp_port)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("绑定 0.0.0.0:{} 失败: {}", o.tcp_port, e);
            return EXIT_OPEN;
        }
    };
    eprintln!("[nn] UDP 0.0.0.0:{} 监听中（Ctrl+C 退出）", o.tcp_port);
    eprintln!("[nn] 日志文件: {}", log_path.display());
    sock.set_read_timeout(Some(Duration::from_millis(100))).ok();
    let started = Instant::now();
    let mut sink = Sink::new(&o, &log_path, started);
    let mut lines = 0usize;
    // UDP 单包上限 65507，缓冲必须足够大，否则 >4096 的数据报会被内核静默截断
    let mut buf = [0u8; 65535];
    loop {
        if limits_reached(&o, lines, started) {
            break;
        }
        match sock.recv_from(&mut buf) {
            Ok((n, from)) => {
                eprintln!("[nn] 来自 {}:{}", from, n); // 状态走 stderr，stdout 保持纯数据
                lines += sink.feed(&buf[..n]);
            }
            Err(ref e)
                if e.kind() == io::ErrorKind::TimedOut || e.kind() == io::ErrorKind::WouldBlock => {
            }
            Err(e) => {
                eprintln!("[nn] 读取错误: {}", e);
                return EXIT_OPEN;
            }
        }
    }
    EXIT_OK
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(|s| s.as_str()) {
        Some("ports") => cmd_ports(),
        Some("install-skill") => cmd_install_skill(),
        Some("version") => cmd_version(&args[1..]),
        Some("check-update") => cmd_check_update(&args[1..]),
        Some("examples") | Some("example") | Some("--examples") => cmd_examples(),
        Some("log") => cmd_log(&args[1..]),
        Some("send") => cmd_send(&args[1..]),
        Some("tcp") => cmd_tcp(&args[1..]),
        Some("udp") => cmd_udp(&args[1..]),
        Some("help") => cmd_help(args.get(1).map(|s| s.as_str())),
        Some("--version") | Some("-V") => cmd_version(&args[1..]),
        Some("--help") | Some("-h") | None => {
            print_usage();
            EXIT_OK
        }
        other => {
            eprintln!("未知命令: {:?}", other);
            print_usage();
            EXIT_ARGS
        }
    };
    std::process::exit(code);
}

// ============ 单元测试（cargo test --bin nn） ============
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_payload_ok() {
        assert_eq!(
            parse_output_payload("01 03 00 00", true).unwrap(),
            vec![0x01, 0x03, 0x00, 0x00]
        );
        assert_eq!(
            parse_output_payload("0a0B", true).unwrap(),
            vec![0x0a, 0x0b]
        );
        assert_eq!(parse_output_payload("AT", false).unwrap(), b"AT".to_vec());
    }

    #[test]
    fn hex_payload_invalid_rejected() {
        // 奇数长度 / 非法字符 / 空内容都必须报错，不能静默丢字节或 panic
        assert!(parse_output_payload("012", true).is_err());
        assert!(parse_output_payload("01 0G", true).is_err());
        assert!(parse_output_payload("全角１２", true).is_err());
        assert!(parse_output_payload("  ", true).is_err());
    }

    #[test]
    fn version_compare() {
        assert!(version_gt("0.6.3", "0.6.2"));
        assert!(version_gt("v0.7.0", "0.6.9"));
        assert!(version_gt("1.0.0", "0.99.99"));
        assert!(!version_gt("0.6.2", "0.6.2"));
        assert!(!version_gt("0.6.2-beta.1", "0.6.2"));
        assert!(!version_gt("0.6.2", "0.6.3"));
        assert!(!version_gt("bad", "0.6.2"));
        assert_eq!(parse_version("v1.2"), Some((1, 2, 0)));
    }

    #[test]
    fn skill_md_has_new_commands_and_valid_send_example() {
        assert!(SKILL_MD.contains("nn version"));
        assert!(SKILL_MD.contains("nn check-update"));
        assert!(SKILL_MD.contains("nn examples"));
        assert!(SKILL_MD.contains("nn help"));
        // 回归：历史版本这里曾把 \"AT 与真实换行写进 raw string，AI 读到的示例是坏的
        assert!(SKILL_MD.contains(r#"-d "AT\r\n""#));
        assert!(!SKILL_MD.contains("\"AT\n"));
    }

    fn temp_test_dir(tag: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("nn-log-test-{}-{}", tag, stamp))
    }

    #[test]
    fn out_existing_directory_gets_generated_file_name() {
        let dir = temp_test_dir("existing");
        fs::create_dir_all(&dir).unwrap();
        let path = resolve_out_path(dir.to_str().unwrap(), "COM3-123.log").unwrap();
        assert_eq!(path.parent(), Some(dir.as_path()));
        assert_eq!(path.file_name().unwrap(), "COM3-123.log");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn out_missing_directory_without_extension_is_created() {
        let dir = temp_test_dir("newdir");
        assert!(!dir.exists());
        let path = resolve_out_path(dir.to_str().unwrap(), "COM3-123.log").unwrap();
        assert!(dir.is_dir());
        assert_eq!(path, dir.join("COM3-123.log"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn out_file_path_kept_and_parent_created() {
        let base = temp_test_dir("file");
        let file = base.join("nested").join("my.log");
        let path = resolve_out_path(file.to_str().unwrap(), "ignored.log").unwrap();
        assert_eq!(path, file);
        assert!(path.parent().unwrap().is_dir());
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn out_trailing_separator_is_directory() {
        let dir = temp_test_dir("trailing");
        let raw = format!("{}{}", dir.display(), std::path::MAIN_SEPARATOR);
        let path = resolve_out_path(&raw, "UDP-9000-1.log").unwrap();
        assert!(dir.is_dir());
        assert_eq!(path, dir.join("UDP-9000-1.log"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn out_empty_rejected() {
        assert!(resolve_out_path("   ", "x.log").is_err());
    }
}
