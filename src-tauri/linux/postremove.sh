#!/bin/sh
# NNSerialTool deb 卸载脚本：仅在真正移除（remove/purge）时清理全局链接。
# 升级（upgrade）阶段不删——新包的 postinstall 会立即重建。
if [ "$1" = "remove" ] || [ "$1" = "purge" ]; then
  rm -f /usr/local/bin/nn
  echo "postrm: 已移除 /usr/local/bin/nn 链接"
fi
