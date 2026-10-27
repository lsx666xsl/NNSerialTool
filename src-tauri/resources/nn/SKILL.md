---
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
- `nn log ... -o D:\path\` — 目录：自动生成 `COM3-<时间戳>.log`，目录不存在会创建
- `nn log ... -o D:\path\device.log` — 文件：原样写入该文件（父目录不存在会创建）
- `nn send -p COM3 -b 115200 -d "AT\r\n"` — 发送文本（-x 为 HEX 字节）
- `nn tcp --host 192.168.1.10 --port 9000 -w -n 100` — TCP 设备
- `nn udp --port 9000 -w -n 50` — UDP 接收
- `nn version [--json]` — 查看 nn / 应用 / NN-Wave 协议版本
- `nn check-update [--json]` — 检查是否有新版本（退出码 0=最新，10=有更新，4=检查失败）
- `nn examples` — 查看常用命令示例（串口/TCP/UDP/HEX/自动采集）
- `nn help [子命令]` — 查看某个子命令的参数与示例（用法中 `<必选>` / `[可选]` 有标记）
- `nn install-skill` — 重装本 SKILL 文件到 ~/.agents/skills/nn/

## 退出码

0 成功 / 2 参数错误 / 3 打开失败（含端口被占用）/ 4 检查更新失败 / 10 发现新版本

## 注意

- 参数标记：`<必选>` 必须填写，`[可选]` 可省略；不确定时先跑 `nn examples` 或 `nn help <子命令>`
- `-o/--out`：目录（已存在、以分隔符结尾、或无扩展名的不存在路径）→ 自动命名 `<端口>-<时间戳>.log`；带扩展名的文件路径 → 原样写入；两种都会自动创建缺失目录
- Windows 串口独占：GUI 打开同一端口时 nn 打不开——先在界面断开连接
- 数据只在 stdout；连接状态/丢帧统计在 stderr——重定向时只取 stdout 即为纯数据
- 通道颜色/波形显示等界面功能在 NNSerialTool 主程序的波形视图中（市场插件）
