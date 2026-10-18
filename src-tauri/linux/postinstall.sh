#!/bin/sh
# NNSerialTool deb 安装后脚本（root 运行）：把随包附带的 nn CLI 符号链接进全局 PATH。
# 资源安装路径随 Tauri 版本/配置而异——按候选列表探测，找不到再用 find 兜底。
# 注意：deb 的 postinst 以 root 身份执行，$HOME 可能是 /root；本脚本不依赖 $HOME。
set -e

NN_SRC=""
for c in \
  "/usr/lib/NNSerialTool/resources/nn" \
  "/usr/lib/NNSerialTool/resources/target/release/nn" \
  "/usr/lib/serialtool/resources/nn" \
  "/usr/lib/serialtool/resources/target/release/nn" \
  "/opt/NNSerialTool/resources/nn" \
  "/opt/NNSerialTool/resources/target/release/nn"; do
  if [ -f "$c" ]; then
    NN_SRC="$c"
    break
  fi
done

if [ -z "$NN_SRC" ]; then
  # 兜底深度到 6：/usr/lib/<产品>/resources/target/release/nn 共 5 层，
  # 留 1 层余量；先找 target/release 再找平铺的 resources/nn。
  NN_SRC=$(find /usr/lib -maxdepth 6 \
    \( -path "*resources/target/release/nn" -o -path "*resources/nn" \) \
    -type f 2>/dev/null | head -1)
fi

if [ -z "$NN_SRC" ]; then
  echo "postinstall: 未找到 nn 二进制（跳过全局命令集成）" >&2
  exit 0
fi

chmod 755 "$NN_SRC" 2>/dev/null || true
mkdir -p /usr/local/bin
ln -sfn "$NN_SRC" /usr/local/bin/nn
echo "postinstall: nn 命令已链接到 /usr/local/bin/nn（新开终端可用）"
