#!/bin/sh
# NNSerialTool deb 安装后脚本（root 运行）：把随包附带的 nn CLI 符号链接进全局 PATH。
# 资源安装路径随 Tauri 版本/配置而异——按候选列表探测，找不到再用 find 兜底。
set -e

NN_SRC=""
for c in \
  "/usr/lib/NNSerialTool/resources/nn" \
  "/usr/lib/serialtool/resources/nn" \
  "/opt/NNSerialTool/resources/nn"; do
  if [ -f "$c" ]; then
    NN_SRC="$c"
    break
  fi
done

if [ -z "$NN_SRC" ]; then
  NN_SRC=$(find /usr/lib -maxdepth 4 -path "*resources/nn" -type f 2>/dev/null | head -1)
fi

if [ -z "$NN_SRC" ]; then
  echo "postinstall: 未找到 nn 二进制（跳过全局命令集成）" >&2
  exit 0
fi

chmod 755 "$NN_SRC" 2>/dev/null || true
mkdir -p /usr/local/bin
ln -sfn "$NN_SRC" /usr/local/bin/nn
echo "postinstall: nn 命令已链接到 /usr/local/bin/nn（新开终端可用）"
