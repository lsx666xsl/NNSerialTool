#!/bin/sh
# NNSerialTool deb 卸载脚本：仅在真正移除（remove/purge）时清理全局链接与 AI SKILL。
# 升级（upgrade）阶段不删——新包的 postinstall 会立即重建。
#
# AI SKILL 说明：nn install-skill 会把 SKILL 写进"运行该命令的用户"的
# ~/.agents/skills/nn/。deb 卸载脚本以 root 运行，$HOME 通常是 /root，
# 因此这里只能清掉 root 自己目录下那份（若存在）；普通用户目录下的副本
# 需用户自行删除——这是 deb maintainer script 拿不到登录用户 HOME 的固有限制。
if [ "$1" = "remove" ] || [ "$1" = "purge" ]; then
  rm -f /usr/local/bin/nn
  echo "postrm: 已移除 /usr/local/bin/nn 链接"
  rm -rf "${HOME:-/root}/.agents/skills/nn"
  echo "postrm: 已清理 ${HOME:-/root}/.agents/skills/nn（其他用户目录需自行删除）"
fi
