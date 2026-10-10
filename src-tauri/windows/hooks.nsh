; NNSerialTool NSIS 安装钩子（tauri.conf.json bundle.windows.nsis.installerHooks 引用）
; Tauri v2 规范：模板已内置 .onInstSuccess 等回调，自定义逻辑必须写在
; NSIS_HOOK_PREINSTALL / NSIS_HOOK_POSTINSTALL / NSIS_HOOK_POSTUNINSTALL 宏里，
; 由模板在对应阶段插入（自定义 .onInstSuccess 会与模板重复定义而中止打包）。
; 职责：安装成功后询问是否将 nn 命令行工具（随安装包附带）加入用户 PATH。
; 说明：
;   - 仅写 HKCU（当前用户），不需要管理员权限
;   - 幂等：PATH 已包含安装目录时跳过，重复安装不会造成无限追加
;   - 广播 WM_SETTINGCHANGE，新开的终端立即可用（已开的终端需重开）

!include "StrFunc.nsh"

!macro NSIS_HOOK_POSTINSTALL
  ; ① 安装 AI SKILL 文件到 ~/.agents/skills/nn/（无条件，体积小，供 AI 自动发现）
  ExecWait '"$INSTDIR\nn.exe" install-skill'
  ; ② 询问是否将 nn 加入用户 PATH
  ReadRegStr $0 HKCU "Environment" "Path"
  StrCpy $1 "$INSTDIR"
  ; 已包含安装目录 → 跳过（幂等）
  ${StrLoc} $2 $0 $1 ">"
  ${If} $2 != ""
    Goto nn_path_done
  ${EndIf}
  ; 询问用户（选项：是/否）
  MessageBox MB_YESNO|MB_ICONQUESTION "是否将 nn 命令行工具添加到用户 PATH 环境变量？$\n$\n（新开的终端即可直接使用 nn ports / nn log 等命令）" IDYES nn_path_add
  Goto nn_path_done

nn_path_add:
  ${If} $0 == ""
    WriteRegStr HKCU "Environment" "Path" "$INSTDIR"
  ${Else}
    WriteRegStr HKCU "Environment" "Path" "$0;$INSTDIR"
  ${EndIf}
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment"

nn_path_done:
!macroend
