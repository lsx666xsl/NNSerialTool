//! NN-Wave v1 —— 轻量二进制波形协议（发送端，Rust 版）
//!
//! 与固件 C 模板（nnwave.c/h）严格同协议，帧格式：
//! `[AA 55][type][N][seq][float32×N 小端][CRC8]`
//! - type 0x01 = 数据帧（N 个 float32）
//! - type 0x02 = 通道名元数据帧（payload 为若干 `[len][utf8]`，补 0x00 到 4N 字节）
//! - CRC8 多项式 0x07，初值 0x00，覆盖 type..data
//!
//! 上位机：NNSerialTool 波形插件（协议选 NN-Wave）。
//! **最多 64 通道**（帧内 N 为 1 字节，本实现与上位机解析器统一按 64 校验）。
//!
//! 设计约束：
//! - 零动态内存：不使用 String/Vec/alloc，帧缓冲在栈上（6 + 4×64 = 262 字节），
//!   `#![no_std]` 固件直接 `mod nnwave;` 即可；
//! - 小端假设：常见 MCU（ARM/x86/RISC-V 小端核）`f32::to_le_bytes` 零开销，
//!   大端核（极少见）该函数会自动逐字节装填，无需修改。

#![allow(dead_code)] //协议三件套常随裁剪只用其一，不按未用告警

/// 通道数硬上限：帧内 N 字段 1 字节 + 上位机解析器同值校验，两端一致
pub const NNWAVE_MAX_CHANNELS: usize = 64;

/// 数据帧类型
pub const NNWAVE_TYPE_DATA: u8 = 0x01;
/// 通道名元数据帧类型
pub const NNWAVE_TYPE_NAMES: u8 = 0x02;

/// 阻塞发送回调 trait：实现到 UART/SPI/USB-CDC 发送包装上
/// 返回 0 表示成功（与 C 模板 `int (*write)(const uint8_t*, size_t)` 对齐）
pub trait NnWaveWrite {
    fn write(&mut self, data: &[u8]) -> i32;
}

/// NN-Wave 发送端：持有一个发送回调的可变借用 + 自动递增的帧序号
pub struct NnWave<'a> {
    writer: &'a mut dyn NnWaveWrite,
    seq: u8,
}

impl<'a> NnWave<'a> {
    /// 创建发送端：注入发送回调
    pub fn new(writer: &'a mut dyn NnWaveWrite) -> Self {
        Self { writer, seq: 0 }
    }

    /// 发送一帧数据：channels[0..n] 对应波形 CH1..CHn（n ≤ 64，超出返回 -1）
    pub fn send(&mut self, channels: &[f32]) -> i32 {
        if channels.is_empty() || channels.len() > NNWAVE_MAX_CHANNELS {
            return -1;
        }
        let mut buf = [0u8; 6 + 4 * NNWAVE_MAX_CHANNELS];
        let n = channels.len();
        buf[0] = 0xAA;
        buf[1] = 0x55;
        buf[2] = NNWAVE_TYPE_DATA;
        buf[3] = n as u8;
        buf[4] = self.seq;
        self.seq = self.seq.wrapping_add(1); //帧序号自动回绕递增（上位机据此统计丢帧）
        for (i, v) in channels.iter().enumerate() {
            buf[5 + i * 4..9 + i * 4].copy_from_slice(&v.to_le_bytes());
        }
        let len = 6 + 4 * n;
        buf[len - 1] = crc8(&buf[2..len - 1]);
        self.writer.write(&buf[..len])
    }

    /// 发送通道名（可选）：上电发一次，波形图例将显示这些名字。
    /// 单名最长 62 字节（留 1 字节长度位），超长截断，总容量受 4N 字节限制
    pub fn send_names(&mut self, names: &[&str]) -> i32 {
        if names.is_empty() || names.len() > NNWAVE_MAX_CHANNELS {
            return -1;
        }
        let n = names.len();
        let mut payload = [0u8; 4 * NNWAVE_MAX_CHANNELS];
        let mut p = 0usize;
        for name in names {
            let mut bytes = name.as_bytes();
            if bytes.len() > 62 {
                bytes = &bytes[..62]; //单名最长 62 字节
            }
            if p + 1 + bytes.len() > 4 * n {
                break; //总容量受 4N 字节限制
            }
            payload[p] = bytes.len() as u8;
            p += 1;
            payload[p..p + bytes.len()].copy_from_slice(bytes);
            p += bytes.len();
        }
        self.frame(NNWAVE_TYPE_NAMES, n, &payload)
    }

    /// 组帧并发送（公开给需要自定义 payload 的高级用户）
    fn frame(&mut self, frame_type: u8, n: usize, payload: &[u8]) -> i32 {
        let mut buf = [0u8; 6 + 4 * NNWAVE_MAX_CHANNELS];
        buf[0] = 0xAA;
        buf[1] = 0x55;
        buf[2] = frame_type;
        buf[3] = n as u8;
        buf[4] = self.seq;
        self.seq = self.seq.wrapping_add(1);
        buf[5..5 + 4 * n].copy_from_slice(&payload[..4 * n]);
        let len = 6 + 4 * n;
        buf[len - 1] = crc8(&buf[2..len - 1]);
        self.writer.write(&buf[..len])
    }
}

/// CRC8：多项式 0x07，初值 0x00，覆盖 type..data（与上位机解析器一致）
pub fn crc8(data: &[u8]) -> u8 {
    let mut crc: u8 = 0;
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
