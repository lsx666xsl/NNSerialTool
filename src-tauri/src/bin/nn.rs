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
//
// --nnwave：按 NN-Wave 协议解码输出 CSV（t_ms,ch1,ch2,...）——解码为 nn 内置实现，
// 与波形视图 / 固件模板 nnwave.c 同协议（AA 55 | type | N | seq | f32×N | CRC8），
// 无需安装任何波形插件。
use serialport::SerialPortType;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::net::{TcpStream, UdpSocket};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const EXIT_OK: i32 = 0;
const EXIT_ARGS: i32 = 2;
const EXIT_OPEN: i32 = 3;
const MAX_CHANNELS: usize = 64;

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

参数:
  -p / --port <口>       串口名（log/send 用：Windows COM3 / Linux /dev/ttyUSB0 /
                         macOS /dev/tty.usbmodem*）；tcp/udp 子命令下为网络端口
  -b / --baud <速率>     波特率（默认 115200）
  --lines <N>            读满 N 行后退出
  --duration <秒>        持续 N 秒后退出（可与 --lines 组合，先到先退）
  --hex                  以十六进制输出原始字节
  --nnwave               按 NN-Wave 协议解码为 CSV: t_ms,ch1,ch2,...
                         （解码为 nn 内置，无需安装波形插件）
  -d / --data <内容>     发送内容（--hex 时为 HEX 字符串）
  --out <文件>           日志文件路径（默认 exe 同级 NNlog/<来源>-<时间戳>.log，自动创建目录）

退出码: 0 成功 / 2 参数错误 / 3 打开失败"
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
            args.get(*i).cloned().ok_or_else(|| format!("参数 {} 缺少值", name))
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
    Text { line: String },
    Hex,
    NnWave { buf: Vec<u8>, seq_last: i32, t0: Instant },
}

struct Sink {
    kind: SinkKind,
    log: Option<File>,
}

impl Sink {
    fn new(o: &Opts, log_path: &PathBuf, t0: Instant) -> Sink {
        let log = OpenOptions::new().create(true).append(true).open(log_path).ok();
        let kind = if o.nnwave {
            SinkKind::NnWave { buf: Vec::new(), seq_last: -1, t0 }
        } else if o.hex {
            SinkKind::Hex
        } else {
            SinkKind::Text { line: String::new() }
        };
        Sink { kind, log }
    }

    // 喂一段原始字节；返回输出的行数（供 --lines 退出条件使用）
    fn feed(&mut self, data: &[u8]) -> usize {
        let mut emitted = 0usize;
        let texts: Vec<String> = match &mut self.kind {
            SinkKind::Text { line } => {
                let mut out = Vec::new();
                for &b in data {
                    if b == b'\n' {
                        out.push(line.trim_end_matches('\r').to_string());
                        line.clear();
                        emitted += 1;
                    } else {
                        // 设备日志按字节透传（UTF-8 多字节在行内自然拼接）
                        line.push(b as char);
                    }
                }
                out
            }
            SinkKind::Hex => {
                if data.is_empty() {
                    Vec::new()
                } else {
                    emitted = 1;
                    vec![data.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ")]
                }
            }
            SinkKind::NnWave { buf, seq_last, t0: start } => {
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
                writeln!(f, "{}", text).ok();
            }
        }
        io::stdout().flush().ok();
        if let Some(f) = &mut self.log {
            f.flush().ok();
        }
        emitted
    }
}

fn nnwave_crc8(data: &[u8]) -> u8 {
    let mut crc = 0u8;
    for &b in data {
        crc ^= b;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 { (crc << 1) ^ 0x07 } else { crc << 1 };
        }
    }
    crc
}

// ---------- 日志文件路径：--out 优先，否则 exe 同级 NNlog/<来源>-<时间戳>.log ----------
fn resolve_log_path(o: &Opts, source: &str) -> Result<PathBuf, String> {
    if let Some(p) = &o.out {
        let path = PathBuf::from(p);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建日志目录失败: {}", e))?;
        }
        return Ok(path);
    }
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs();
    let safe: String = source
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
        .collect();
    let name = format!("{}-{}.log", safe, ts);
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
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    let dir = PathBuf::from(home).join("NNlog");
    fs::create_dir_all(&dir).map_err(|e| format!("创建日志目录失败: {}", e))?;
    Ok(dir.join(name))
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
const SKILL_MD: &str = r#"---
name: nn
description: NNSerialTool 命令行工具——读取串口/TCP/UDP 设备日志与 NN-Wave 数值数据流。当用户要求读取串口输出、查看设备日志、抓取传感器数值、监听 TCP/UDP 数据流、或需要硬件实时数据进行分析时使用。
---

# nn — 设备数据读取 CLI

独立可执行文件（随 NNSerialTool 安装）。stdout 只输出数据，状态/错误走 stderr，适合 AI 直接消费；所有输出同步落盘到 exe 同级 NNlog/ 目录。

## 命令速查

- `nn ports` — 列出串口（含 USB vid/pid）
- `nn log -p COM3 -b 115200 -n 200` — 读 200 行设备输出后退出
- `nn log -p COM3 -b 115200 -t 10` — 采集 10 秒
- `nn log -p COM3 -b 115200 -w -t 10` — NN-Wave 解码为 CSV（t_ms,ch1,ch2,...），无需安装波形插件
- `nn log -p COM3 -b 115200 -x -n 20` — HEX 字节流
- `nn log ... -o D:\path\device.log` — 同步落盘到指定文件
- `nn send -p COM3 -b 115200 -d \"AT
\"` — 发送文本（-x 为 HEX 字节）
- `nn tcp --host 192.168.1.10 --port 9000 -w -n 100` — TCP 设备
- `nn udp --port 9000 -w -n 50` — UDP 接收
- `nn install-skill` — 重装本 SKILL 文件到 ~/.agents/skills/nn/

## 退出码

0 成功 / 2 参数错误 / 3 打开失败（含端口被占用）

## 注意

- Windows 串口独占：GUI 打开同一端口时 nn 打不开——先在界面断开连接
- 数据只在 stdout；连接状态/丢帧统计在 stderr——重定向时只取 stdout 即为纯数据
- 通道颜色/波形显示等界面功能在 NNSerialTool 主程序的波形视图中（市场插件）
"#;

fn cmd_install_skill() -> i32 {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    if home.is_empty() {
        eprintln!("[nn] 无法定位用户目录（USERPROFILE/HOME 均未设置）");
        return EXIT_OPEN;
    }
    let dir = std::path::Path::new(&home).join(".agents").join("skills").join("nn");
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

// 分命令详细帮助：nn help [log|send|tcp|udp|ports]
fn cmd_help(topic: Option<&str>) -> i32 {
    match topic {
        Some("log") => println!(
            "nn log —— 实时读取串口输出（数据走 stdout，状态走 stderr）

用法:
  nn log -p COM3 [-b 115200] [过滤/格式参数]

参数:
  -p <口>        串口名
  -b <速率>      波特率（默认 115200）
  -n <N>         读满 N 行退出
  -t <秒>        采集 N 秒退出
  -w             NN-Wave 解码为 CSV: t_ms,ch1,ch2,...（内置解码，无需波形插件）
  -x             十六进制输出原始字节
  -o <文件>      日志落盘路径（默认 exe 同级 NNlog/<来源>-<时间戳>.log）

示例:
  nn log -p COM3 -w -t 10        采集 10 秒并解码为 CSV
  nn log -p COM3 -n 200          读 200 行日志"),
        Some("send") => println!(
            "nn send —— 发送文本或 HEX 字节

用法:
  nn send -p COM3 [-b 115200] -d <内容> [-x]

示例:
  nn send -p COM3 -d \"AT\r\n\"
  nn send -p COM3 -x -d \"01 03 00 00 00 02\""),
        Some("tcp") => println!(
            "nn tcp —— 读取 TCP 设备日志

用法:
  nn tcp -H <地址> --port <端口> [-w] [-x] [-n N] [-t 秒] [-o 文件]

示例:
  nn tcp -H 127.0.0.1 --port 9000 -w -n 100"),
        Some("udp") => println!(
            "nn udp —— 接收 UDP 数据报

用法:
  nn udp --port <本地端口> [-w] [-x] [-n N] [-t 秒]"),
        Some("ports") | None => print_usage(),
        Some(other) => {
            println!("未知主题: {}（可选 log/send/tcp/udp/ports）", other);
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
        .map_err(|e| (format!("打开 {} 失败: {}（若被 GUI 占用请先在界面关闭连接）", o.port, e), EXIT_OPEN))
}

fn cmd_log(args: &[String]) -> i32 {
    let o = match parse_opts(args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("参数错误: {}", e);
            return EXIT_ARGS;
        }
    };
    let log_path = match resolve_log_path(&o, &format!("{}-{}", o.port, o.baud)) {
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
            Ok(0) => {}
            Ok(n) => {
                lines += sink.feed(&buf[..n]);
            }
            Err(ref e) if e.kind() == io::ErrorKind::TimedOut || e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => {
                eprintln!("[nn] 读取错误: {}", e);
                break;
            }
        }
    }
    EXIT_OK
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
    let mut port = match open_serial(&o) {
        Ok(p) => p,
        Err((msg, code)) => {
            eprintln!("{}", msg);
            return code;
        }
    };
    let payload: Vec<u8> = if o.hex {
        let cleaned: String = o.data.chars().filter(|c| !c.is_whitespace()).collect();
        (0..cleaned.len() / 2)
            .filter_map(|i| u8::from_str_radix(&cleaned[i * 2..i * 2 + 2], 16).ok())
            .collect()
    } else {
        o.data.clone().into_bytes()
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
    let log_path = match resolve_log_path(&o, &format!("{}-{}", o.host, o.tcp_port)) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{}", e);
            return EXIT_OPEN;
        }
    };
    let mut stream = match TcpStream::connect((o.host.as_str(), o.tcp_port)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("连接 {}:{} 失败: {}", o.host, o.tcp_port, e);
            return EXIT_OPEN;
        }
    };
    eprintln!("[nn] {}:{} 已连接（Ctrl+C 退出）", o.host, o.tcp_port);
    eprintln!("[nn] 日志文件: {}", log_path.display());
    stream.set_read_timeout(Some(Duration::from_millis(100))).ok();
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
            Err(ref e) if e.kind() == io::ErrorKind::TimedOut || e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => {
                eprintln!("[nn] 读取错误: {}", e);
                break;
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
    let mut buf = [0u8; 4096];
    loop {
        if limits_reached(&o, lines, started) {
            break;
        }
        match sock.recv_from(&mut buf) {
            Ok((n, from)) => {
                eprintln!("[nn] 来自 {}:{}", from, n); // 状态走 stderr，stdout 保持纯数据
                lines += sink.feed(&buf[..n]);
            }
            Err(ref e) if e.kind() == io::ErrorKind::TimedOut || e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => {
                eprintln!("[nn] 读取错误: {}", e);
                break;
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
        Some("log") => cmd_log(&args[1..]),
        Some("send") => cmd_send(&args[1..]),
        Some("tcp") => cmd_tcp(&args[1..]),
        Some("udp") => cmd_udp(&args[1..]),
        Some("help") => cmd_help(args.get(1).map(|s| s.as_str())),
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
