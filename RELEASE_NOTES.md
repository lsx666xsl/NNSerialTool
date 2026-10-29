【修复】
- 修复串口端口下拉在长设备名时撑出新建会话面板：表单网格改为 minmax(0,1fr)，SelfSelect/标签补 min-width:0（下拉浮层已有面板边界钳制）
- 含 v0.6.9 修复：nn --out 目录/文件、安装/卸载卡死、旧版升级路径、nn/SKILL 校验、标签开关性能、状态圆点与拖拽热区
【新功能】
- nn 默认输出每行时间戳 `[YYYY-MM-DD HH:mm:ss]`；`--no-timestamp` 可关闭（CSV/AI 管道建议关闭）
- 音视频解析插件增加专用悬停预览图（文件卡片 + 码流 + HEX，插件上架后生效）
【优化】
- macOS 包使用完整 ad-hoc app bundle 签名，避免下载后报“已损坏”；未公证，首次打开仍需右键/系统设置允许
- nn examples/help 补充时间戳、目录/文件输出示例

---
由 NNSerialTool 发布流水线自动维护
