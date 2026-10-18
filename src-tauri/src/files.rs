/*
 * 插件文件桥：WebView 沙箱里的 JS 没有磁盘权限，插件系统的文件读写统一走这几个命令。
 * - plugin_* 系列锚定 app_data/plugins/<插件id>/，rel_path 走白名单校验防目录穿越；
 * - plugin_install 一次事务写入插件全部文件（staging + rename，失败不留半成品）；
 * - import_plugin_zip 只读"用户刚在原生对话框里选中的 zip 包"（本地导入插件包，用户已知情选择）；
 * - export_text_files 覆盖写多个文本文件（固件协议文件导出），与 serial_log_write 的追加语义不同。
 */
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

// 插件安装上限：文件数与总字节数（防止前端误传超大/超多文件把磁盘塞满）
const MAX_INSTALL_FILES: usize = 500;
const MAX_PLUGIN_BYTES: usize = 32 * 1024 * 1024;
// 导入插件包（zip）大小上限，与安装总大小上限保持一致
const MAX_IMPORT_ZIP_BYTES: u64 = 32 * 1024 * 1024;

/*
 * 描述：插件安装的单个文件（前端传 relPath / rel_path，值为相对插件目录的路径）
 */
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallFile {
    #[serde(alias = "rel_path")]
    rel_path: String,
    bytes: Vec<u8>,
}

/*
 * 描述：导入的插件包（zip 原始字节，解压由前端完成；体积大，不回传路径）
 */
#[derive(Serialize)]
pub struct ImportZip {
    name: String,
    bytes: Vec<u8>,
}

/*
 * 描述：导出的单个文本文件
 */
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportFile {
    name: String,
    text: String,
}

fn plugins_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("定位应用数据目录失败: {e}"))?
        .join("plugins");
    fs::create_dir_all(&dir).map_err(|e| format!("创建插件目录失败: {e}"))?;
    Ok(dir)
}

/*
 * 描述：相对路径白名单：统一分隔符后逐段校验，返回规范化后的 PathBuf。
 *       拒绝绝对路径/盘符/UNC/开头分隔符、空段、"." 与 ".." 段、超深层级，
 *       段名只允许 [a-zA-Z0-9._-]；Windows 不允许以点/空格结尾的段名（会被系统裁剪造成同名覆盖）也拒绝。
 */
fn safe_rel(rel: &str) -> Result<PathBuf, String> {
    let normalized = rel.replace('\\', "/");
    // 绝对路径（Unix "/xxx"）与 UNC（"//server/share"）直接拒绝
    if normalized.is_empty() || normalized.starts_with('/') {
        return Err("非法插件路径".into());
    }
    let mut out = PathBuf::new();
    let mut seg_count = 0usize;
    for seg in normalized.split('/') {
        // 空段（"a//b"、结尾 "/"）一律拒绝，避免规范化歧义
        if seg.is_empty() {
            return Err("非法插件路径".into());
        }
        if seg == "." || seg == ".." {
            return Err(format!("非法插件路径段: {seg}"));
        }
        // 只允许字母/数字/点/下划线/连字符：盘符 "C:"、冒号、通配符等一律拒绝
        if !seg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        {
            return Err(format!("非法插件路径段: {seg}"));
        }
        if seg.ends_with('.') || seg.ends_with(' ') {
            return Err(format!("非法插件路径段: {seg}"));
        }
        // Windows 保留设备名（CON/PRN/AUX/NUL/COM1-9/LPT1-9）在任意目录下都不能作为文件名，
        // 否则写入行为由系统决定（可能指向设备或直接失败）。大小写不敏感。
        let upper = seg.to_ascii_uppercase();
        let base = upper.split('.').next().unwrap_or(&upper);
        if matches!(base, "CON" | "PRN" | "AUX" | "NUL")
            || (base.len() == 4
                && (base.starts_with("COM") || base.starts_with("LPT"))
                && base.as_bytes()[3].is_ascii_digit()
                && base.as_bytes()[3] != b'0')
        {
            return Err(format!("非法插件路径段（Windows 保留名）: {seg}"));
        }
        out.push(seg);
        seg_count += 1;
    }
    if seg_count > 8 {
        return Err("非法插件路径".into());
    }
    Ok(out)
}

/*
 * 描述：单段安全名（插件 id / 导出文件夹名）：必须是 safe_rel 允许的单个路径段。
 */
fn safe_leaf(name: &str) -> Result<String, String> {
    let rel = safe_rel(name.trim())?;
    if rel.components().count() != 1 {
        return Err(format!("非法目录名: {name}"));
    }
    // 顶层目录名不允许以 "." 开头：plugin_list_ids 会隐藏点目录，装进去等于“隐形插件”
    let leaf = rel.to_string_lossy().to_string();
    if leaf.starts_with('.') {
        return Err(format!("非法目录名: {name}"));
    }
    Ok(leaf)
}

/*
 * 描述：文件名清洗：Windows 非法字符替换为下划线，去掉首尾空白与结尾点/空格；
 *       清洗后为空或为 "."/".." 视为非法（防止借文件名逃出目标目录）。
 */
fn sanitize_file_name(name: &str) -> Result<String, String> {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned
        .trim()
        .trim_end_matches('.')
        .trim_end_matches(' ')
        .to_string();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        return Err(format!("非法文件名: {name}"));
    }
    Ok(cleaned)
}

/*
 * 描述：读取插件目录下的文本文件
 */
#[tauri::command]
pub async fn plugin_read_text(app: tauri::AppHandle, rel_path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = plugins_root(&app)?;
        let path = root.join(safe_rel(&rel_path)?);
        fs::read_to_string(&path).map_err(|e| format!("读取插件文件失败: {e}"))
    })
    .await
    .map_err(|e| format!("任务执行失败: {e}"))?
}

/*
 * 描述：列出已安装插件目录名（即插件 id）；以 "." 开头的目录是安装临时目录，不对外暴露
 */
#[tauri::command]
pub async fn plugin_list_ids(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = plugins_root(&app)?;
        let mut ids = Vec::new();
        for entry in fs::read_dir(&root)
            .map_err(|e| format!("读取插件目录失败: {e}"))?
            .flatten()
        {
            if entry.path().is_dir() {
                // 只认“含 plugin.json 的目录”为插件：避免用户/其他工具放进 plugins/ 的普通目录
                // 被当成插件列出、甚至被卸载流程删除
                if !entry.path().join("plugin.json").is_file() {
                    continue;
                }
                if let Some(name) = entry.file_name().to_str() {
                    if !name.starts_with('.') && safe_rel(name).is_ok() {
                        ids.push(name.to_string());
                    }
                }
            }
        }
        ids.sort();
        Ok(ids)
    })
    .await
    .map_err(|e| format!("任务执行失败: {e}"))?
}

/*
 * 描述：删除插件目录（卸载）
 *       删除前 canonicalize 消解符号链接/相对片段，校验目标仍在插件根目录内，防止路径逃逸。
 */
#[tauri::command]
pub async fn plugin_remove_dir(app: tauri::AppHandle, id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let root = plugins_root(&app)?;
        let target = root.join(safe_rel(&id)?);
        if !target.exists() {
            return Ok(());
        }
        let root_canon = fs::canonicalize(&root).map_err(|e| format!("定位插件目录失败: {e}"))?;
        let target_canon =
            fs::canonicalize(&target).map_err(|e| format!("定位插件目录失败: {e}"))?;
        if !target_canon.starts_with(&root_canon) {
            return Err("非法插件路径".into());
        }
        if target_canon.is_dir() {
            fs::remove_dir_all(&target_canon).map_err(|e| format!("删除插件目录失败: {e}"))?;
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("任务执行失败: {e}"))?
}

/*
 * 描述：安装插件：先把全部文件写入 plugins/.staging-<id>-<nanos>，
 *       成功后删除旧目录并把 staging rename 成 plugins/<id>（同盘 rename 原子）。
 *       任何一步失败都清理 staging，绝不留下半成品；全部条目先校验再落盘。
 */
#[tauri::command]
pub async fn plugin_install(
    app: tauri::AppHandle,
    id: String,
    files: Vec<InstallFile>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        // ① 先做全部校验：id 单段、文件数/总大小上限、每个 rel_path 合法
        let id_leaf = safe_leaf(&id)?;
        if files.is_empty() {
            return Err("插件文件列表为空".into());
        }
        if files.len() > MAX_INSTALL_FILES {
            return Err(format!(
                "插件文件数量超过上限（最多 {MAX_INSTALL_FILES} 个）"
            ));
        }
        let mut planned: Vec<(PathBuf, &[u8])> = Vec::with_capacity(files.len());
        let mut total = 0usize;
        for file in &files {
            let rel = safe_rel(&file.rel_path)?;
            total = total
                .checked_add(file.bytes.len())
                .ok_or_else(|| "插件文件总大小超出上限".to_string())?;
            if total > MAX_PLUGIN_BYTES {
                return Err(format!(
                    "插件总大小超过上限（最多 {}MB）",
                    MAX_PLUGIN_BYTES / 1024 / 1024
                ));
            }
            planned.push((rel, file.bytes.as_slice()));
        }

        // ② 全部写入临时 staging 目录（同盘 rename 才是原子的）
        let root = plugins_root(&app)?;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let staging = root.join(format!(".staging-{}-{}", id_leaf, nanos));
        let write_result = (|| -> Result<(), String> {
            fs::create_dir_all(&staging).map_err(|e| format!("创建插件临时目录失败: {e}"))?;
            for (rel, data) in &planned {
                let path = staging.join(rel);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(|e| format!("创建插件目录失败: {e}"))?;
                }
                fs::write(&path, *data).map_err(|e| format!("写入插件文件失败: {e}"))?;
            }
            Ok(())
        })();
        if let Err(e) = write_result {
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }

        // ③ 原子替换：先删旧目录，再把 staging rename 成正式目录
        let final_dir = root.join(&id_leaf);
        let replace_result = (|| -> Result<(), String> {
            if final_dir.exists() {
                fs::remove_dir_all(&final_dir).map_err(|e| format!("删除旧插件目录失败: {e}"))?;
            }
            fs::rename(&staging, &final_dir).map_err(|e| format!("安装插件失败: {e}"))?;
            Ok(())
        })();
        if let Err(e) = replace_result {
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("任务执行失败: {e}"))?
}

/*
 * 描述：选择并读取本地插件包（zip），返回文件名与原始字节；用户取消返回 None。
 *       Rust 侧弹原生文件选择框（阻塞式，已在线程池中执行，不卡 UI 主线程）。
 */
#[tauri::command]
pub async fn import_plugin_zip(app: tauri::AppHandle) -> Result<Option<ImportZip>, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<Option<ImportZip>, String> {
        let picked = app
            .dialog()
            .file()
            .set_title("选择插件包")
            .add_filter("插件包", &["zip"])
            .blocking_pick_file();
        let Some(file) = picked else {
            return Ok(None);
        };
        let path = file
            .simplified()
            .into_path()
            .map_err(|e| format!("解析文件路径失败: {e}"))?;
        let size = fs::metadata(&path)
            .map_err(|e| format!("读取文件信息失败: {e}"))?
            .len();
        if size > MAX_IMPORT_ZIP_BYTES {
            return Err(format!(
                "插件包超过 {}MB 上限",
                MAX_IMPORT_ZIP_BYTES / 1024 / 1024
            ));
        }
        let bytes = fs::read(&path).map_err(|e| format!("读取插件包失败: {e}"))?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("plugin.zip")
            .to_string();
        Ok(Some(ImportZip { name, bytes }))
    })
    .await
    .map_err(|e| format!("任务执行失败: {e}"))?
}

/*
 * 描述：导出多个文本文件：Rust 侧弹原生目录选择框，可选在其下新建单段命名的子目录，
 *       逐文件覆盖写（文件名按 Windows 非法字符规则清洗），返回目标目录字符串；用户取消返回 None。
 */
#[tauri::command]
pub async fn export_text_files(
    app: tauri::AppHandle,
    title: String,
    folder_name: Option<String>,
    files: Vec<ExportFile>,
) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<Option<String>, String> {
        if files.is_empty() {
            return Err("没有可导出的文件".into());
        }
        // ① 先校验，全部合法后才弹目录选择框
        let folder = match folder_name {
            Some(name) if !name.trim().is_empty() => Some(safe_leaf(&name)?),
            _ => None,
        };
        let mut planned: Vec<(String, &str)> = Vec::with_capacity(files.len());
        for file in &files {
            planned.push((sanitize_file_name(&file.name)?, file.text.as_str()));
        }

        // ② 目录选择对话框（阻塞式，已在线程池中执行，不卡 UI 主线程）
        let dialog_title = if title.trim().is_empty() {
            "选择导出目录".to_string()
        } else {
            title
        };
        let picked = app
            .dialog()
            .file()
            .set_title(dialog_title)
            .blocking_pick_folder();
        let Some(base) = picked else {
            return Ok(None);
        };
        let base = base
            .simplified()
            .into_path()
            .map_err(|e| format!("解析目录路径失败: {e}"))?;

        let target = match folder {
            Some(name) => base.join(name),
            None => base,
        };
        fs::create_dir_all(&target).map_err(|e| format!("创建导出目录失败: {e}"))?;
        for (name, text) in &planned {
            let path = target.join(name);
            fs::write(&path, text.as_bytes())
                .map_err(|e| format!("写入 {} 失败: {e}", path.display()))?;
        }
        Ok(Some(target.to_string_lossy().to_string()))
    })
    .await
    .map_err(|e| format!("任务执行失败: {e}"))?
}

// ============ 单元测试（cargo test --lib） ============
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_rel_accepts_normal_paths() {
        assert_eq!(
            safe_rel("plugin.json").unwrap(),
            PathBuf::from("plugin.json")
        );
        assert_eq!(
            safe_rel("assets/main.js").unwrap(),
            PathBuf::from("assets").join("main.js")
        );
        assert_eq!(
            safe_rel("a\\b\\c.js").unwrap(),
            PathBuf::from("a").join("b").join("c.js")
        );
        assert!(safe_leaf("wave").is_ok());
        assert!(safe_leaf("wave-1.0").is_ok());
    }

    #[test]
    fn safe_rel_rejects_traversal_and_absolute() {
        for bad in [
            "",
            "/etc/passwd",
            "//server/share/x",
            "C:/Windows/x",
            "..",
            "../x",
            "a/../b",
            "a/./b",
            "a//b",
            "a\\..\\b",
            "a/",
            "a/b/c/d/e/f/g/h/i",
        ] {
            assert!(safe_rel(bad).is_err(), "应拒绝非法路径: {bad:?}");
        }
        // 普通隐藏文件（如 .gitignore）允许作为插件包内文件；仅顶层插件目录名禁止点前缀
        assert!(safe_rel(".hidden/x").is_ok());
        assert!(safe_leaf(".hidden").is_err());
    }

    #[test]
    fn safe_rel_rejects_windows_reserved_names() {
        for bad in [
            "CON", "con.txt", "PRN", "AUX.log", "NUL", "COM1", "com9.js", "LPT1",
        ] {
            assert!(safe_rel(bad).is_err(), "应拒绝 Windows 保留名: {bad:?}");
        }
        assert!(safe_rel("COM0").is_ok());
        assert!(safe_rel("console.log").is_ok());
    }

    #[test]
    fn sanitize_file_name_blocks_escape() {
        assert_eq!(sanitize_file_name("a/b.txt").unwrap(), "a_b.txt");
        assert!(sanitize_file_name("..").is_err());
        assert_eq!(sanitize_file_name("x.").unwrap(), "x");
        assert!(sanitize_file_name("").is_err());
    }
}
