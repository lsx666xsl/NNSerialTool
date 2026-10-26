; NNSerialTool NSIS 安装钩子（tauri.conf.json bundle.windows.nsis.installerHooks 引用）
; Tauri v2 规范：模板已内置 .onInstSuccess 等回调，自定义逻辑必须写在
; NSIS_HOOK_PREINSTALL / NSIS_HOOK_POSTINSTALL / NSIS_HOOK_POSTUNINSTALL 宏里，
; 由模板在对应阶段插入（自定义 .onInstSuccess 会与模板重复定义而中止打包）。
;
; 安装后（POSTINSTALL）：
;   ① 把随包资源 target\release\nn.exe 上移到安装根目录，再删除嵌套副本（带复制失败保护）
;   ② 调用 nn install-skill 安装 AI SKILL 到 ~/.agents/skills/nn/
;   ③ 交互式安装时询问是否把安装目录追加到 HKCU PATH（静默安装不询问、不修改）
; 卸载后（POSTUNINSTALL）：
;   ① 从 HKCU PATH 精确摘除安装目录条目（绝不触碰其他条目）
;   ② 删除 ~/.agents/skills/nn/ 下的 AI SKILL 文件
;
; 关键约束：
;   - 只写 HKCU（当前用户），不需要管理员权限
;   - NSIS 的变量/注册表字符串有 1024 字符上限，而用户 PATH 可能更长（本机实测 >2000）：
;     任何回写前先 StrLen 守卫，接近上限时只提示、不写，避免把 PATH 截断写坏
;   - PATH 追加用 WriteRegExpandStr，保持 REG_EXPAND_SZ 类型（%USERPROFILE% 等仍需展开）
;   - 静默安装（/S）不弹窗、不改 PATH：避免企业批量部署时环境被意外修改
;   - 升级安装（卸载器带 /UPDATE 运行）不清理 PATH/SKILL：新版本安装后继续沿用
;   - 所有修改都广播 WM_SETTINGCHANGE：新开的终端立即生效（已开的终端需重开）
;
; 依赖说明：Tauri NSIS 模板（installer.nsi）已经 !include "StrFunc.nsh"，并实例化了
;   ${StrCase}/${StrLoc}；本文件不重复 include（重复 include 虽被头文件守卫吞掉，
;   但没必要），只实例化自己用到的 ${StrRep}/${UnStrRep}。
;   StrFunc 的函数只能在全局作用域实例化，因此下面两行必须留在文件顶部，不能挪进宏里。

${StrRep}
${UnStrRep}

!macro NSIS_HOOK_POSTINSTALL
  ; ① 资源去重：target\release\nn.exe → 安装根目录。
  ;    必须先确认根目录副本存在，再删嵌套副本：CopyFiles 失败（杀软拦截/磁盘满）时
  ;    嵌套副本是唯一一份，先删会导致 nn 彻底丢失。
  ${If} ${FileExists} "$INSTDIR\target\release\nn.exe"
    CopyFiles /SILENT "$INSTDIR\target\release\nn.exe" "$INSTDIR\nn.exe"
    ${If} ${FileExists} "$INSTDIR\nn.exe"
      Delete "$INSTDIR\target\release\nn.exe"
    ${Else}
      ${IfNot} ${Silent}
        MessageBox MB_ICONEXCLAMATION "nn 命令行工具复制失败，已保留资源目录中的副本。$\n$\n如需命令行，请手动把 $INSTDIR 加入 PATH。"
      ${EndIf}
    ${EndIf}
  ${EndIf}

  ; ② 安装 AI SKILL：直接复制随包资源，不再启动 nn.exe。
  ;    旧实现用 Exec 启动刚解压的 nn.exe，部分杀软实时扫描会阻塞 CreateProcess，
  ;    导致安装器卡在“执行 nn install-skill”（ExecWait 已换 Exec 仍会中招）。
  ;    复制静态文件没有进程启动/扫描阻塞风险；资源缺失时只提示，可手动 nn install-skill。
  ${If} ${FileExists} "$INSTDIR\resources\nn\SKILL.md"
    CreateDirectory "$PROFILE\.agents\skills\nn"
    CopyFiles /SILENT "$INSTDIR\resources\nn\SKILL.md" "$PROFILE\.agents\skills\nn"
    DetailPrint "AI SKILL 已安装到 $PROFILE\.agents\skills\nn\SKILL.md"
  ${Else}
    DetailPrint "未找到 SKILL.md 资源，可稍后在终端运行 nn install-skill"
  ${EndIf}

  ; ③ PATH 询问。静默安装一律跳过：不做静默自动修改，避免企业部署意外
  ${If} ${Silent}
    DetailPrint "静默安装：跳过 PATH 修改（如需命令行请手动添加 $INSTDIR）"
    Goto nn_path_done
  ${EndIf}

  ${IfNot} ${FileExists} "$INSTDIR\nn.exe"
    DetailPrint "nn.exe 不存在，跳过 PATH 集成"
    Goto nn_path_done
  ${EndIf}

  ReadRegStr $0 HKCU "Environment" "Path"

  ; 幂等检查：把现有 PATH 首尾补分号后查完整条目 ";$INSTDIR;"，
  ; 避免把 "C:\...\NNSerialToolBackup" 之类更长的路径误判成"已包含"
  StrCpy $4 ";$0;"
  ${StrRep} $5 "$4" ";$INSTDIR;" ";"
  ${If} $5 != $4
    Goto nn_path_done
  ${EndIf}

  MessageBox MB_YESNO|MB_ICONQUESTION "是否将 nn 命令行工具添加到用户 PATH 环境变量？$\n$\n（新开的终端即可直接使用 nn ports / nn log 等命令）" IDYES nn_path_add
  Goto nn_path_done

nn_path_add:
  ; 长度守卫：NSIS 写入字符串上限 1024 字符，超限会被截断并写坏整个 PATH。
  ; 现有 PATH 接近上限时宁可不动，也不能写坏用户环境。
  StrLen $3 "$0"
  ${If} $3 >= 1000
    MessageBox MB_ICONEXCLAMATION "当前用户 PATH 已接近系统上限（$3 字符），为避免截断损坏环境变量，安装器不会自动修改。$\n$\n请手动把以下路径加入 PATH：$\n$INSTDIR"
    Goto nn_path_done
  ${EndIf}
  ${If} $0 == ""
    StrCpy $6 "$INSTDIR"
  ${Else}
    ; 已有 PATH 以分号结尾时不要再补一个 ";"：Windows 会把空条目当"当前目录"
    StrCpy $7 "$0" 1 -1
    ${If} $7 == ";"
      StrCpy $6 "$0$INSTDIR"
    ${Else}
      StrCpy $6 "$0;$INSTDIR"
    ${EndIf}
  ${EndIf}
  StrLen $5 "$6"
  ${If} $5 >= 1000
    MessageBox MB_ICONEXCLAMATION "追加后用户 PATH 将超过系统上限（$5 字符），为避免截断损坏环境变量，安装器不会自动修改。$\n$\n请手动把以下路径加入 PATH：$\n$INSTDIR"
    Goto nn_path_done
  ${EndIf}
  ; WriteRegExpandStr：保持 REG_EXPAND_SZ，PATH 里的 %USERPROFILE% 等仍可展开
  WriteRegExpandStr HKCU "Environment" "Path" "$6"
  SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment"
  DetailPrint "已把 $INSTDIR 加入用户 PATH"

nn_path_done:
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; 升级安装（卸载器带 /UPDATE）跳过清理：PATH 条目与 SKILL 目录要被新版本继续沿用，
  ; 否则用户升级后还得重新手动加 PATH，SKILL 也得等下次手动重建。
  ${If} $UpdateMode = 1
    DetailPrint "升级安装：保留 nn PATH 条目与 SKILL 目录"
  ${Else}
    ; ① 从用户 PATH 精确摘除安装目录：只删除 ";$INSTDIR;" 边界内的条目，
    ;    其他条目（含以 $INSTDIR 为前缀的更长的路径）原样保留
    ReadRegStr $0 HKCU "Environment" "Path"
    ${If} $0 != ""
      StrLen $3 "$0"
      ${If} $3 >= 1000
        ; 读到的值可能已被 NSIS 截断：此时回写会损坏 PATH，只能提示手动清理
        ${IfNot} ${Silent}
          MessageBox MB_ICONEXCLAMATION "当前用户 PATH 已接近系统上限（$3 字符），为避免截断损坏环境变量，卸载器不会自动移除安装目录。$\n$\n请手动从 PATH 删除：$\n$INSTDIR"
        ${EndIf}
      ${Else}
        ${If} $0 == "$INSTDIR"
          StrCpy $1 ""
        ${Else}
          ; 先删中间/末尾条目（";条目"），再删开头条目（"条目;"），普通条目不受影响
          ${UnStrRep} $1 "$0" ";$INSTDIR" ""
          ${UnStrRep} $1 "$1" "$INSTDIR;" ""
        ${EndIf}
        ${If} $1 != $0
          ${If} $1 == ""
            DeleteRegValue HKCU "Environment" "Path"
          ${Else}
            WriteRegExpandStr HKCU "Environment" "Path" "$1"
          ${EndIf}
          SendMessage ${HWND_BROADCAST} ${WM_SETTINGCHANGE} 0 "STR:Environment"
          DetailPrint "已从用户 PATH 移除 $INSTDIR"
        ${EndIf}
      ${EndIf}
    ${EndIf}

    ; ② 删除 AI SKILL 目录。注意：$PROFILE 是"执行卸载的用户"，
    ;    若由管理员替其他用户卸载，其他用户目录下的副本不会被清理（已知限制）
    RMDir /r "$PROFILE\.agents\skills\nn"
  ${EndIf}
!macroend
