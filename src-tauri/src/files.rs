/*
 * 插件文件桥：WebView 沙箱里的 JS 没有磁盘权限，插件系统的文件读写统一走这几个命令。
 * - plugin_* 系列锚定 app_data/plugins/<插件id>/，rel_path 走白名单校验防目录穿越；
 * - read_dialog_file 只读"用户刚在原生对话框里选中的文件"（本地导入插件包，用户已知情选择）；
 * - write_text_file 覆盖写文本（固件协议文件导出），与 serial_log_write 的追加语义不同。
 */
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

fn plugins_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("定位应用数据目录失败: {e}"))?
        .join("plugins");
    fs::create_dir_all(&dir).map_err(|e| format!("创建插件目录失败: {e}"))?;
    Ok(dir)
}

// 相对路径白名单：段名为 [a-zA-Z0-9._-]+，禁止 ".."、空段、超深层级
fn safe_rel(rel: &str) -> Result<PathBuf, String> {
    let normalized = rel.replace('\\', "/");
    let segs: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();
    if segs.is_empty() || segs.len() > 8 {
        return Err("非法插件路径".into());
    }
    let mut out = PathBuf::new();
    for seg in &segs {
        if *seg == "." || *seg == ".." {
            return Err(format!("非法插件路径段: {seg}"));
        }
        if !seg.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')) {
            return Err(format!("非法插件路径段: {seg}"));
        }
        out.push(seg);
    }
    Ok(out)
}

#[tauri::command]
pub async fn plugin_write_file(app: tauri::AppHandle, rel_path: String, bytes: Vec<u8>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = plugins_root(&app)?;
        let path = root.join(safe_rel(&rel_path)?);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
        }
        fs::write(&path, bytes).map_err(|e| format!("写入插件文件失败: {e}"))
    })
    .await
    .map_err(|e| format!("任务执行失败: {e}"))?
}

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

// 列出已安装插件目录名（即插件 id）
#[tauri::command]
pub async fn plugin_list_ids(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = plugins_root(&app)?;
        let mut ids = Vec::new();
        for entry in fs::read_dir(&root).map_err(|e| format!("读取插件目录失败: {e}"))?.flatten() {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    if safe_rel(name).is_ok() {
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

#[tauri::command]
pub async fn plugin_remove_dir(app: tauri::AppHandle, id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        safe_rel(&id)?;
        let path = plugins_root(&app)?.join(&id);
        if path.is_dir() {
            fs::remove_dir_all(&path).map_err(|e| format!("删除插件目录失败: {e}"))?;
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("任务执行失败: {e}"))?
}

#[tauri::command]
pub async fn read_dialog_file(path: String) -> Result<Vec<u8>, String> {
    tauri::async_runtime::spawn_blocking(move || fs::read(&path).map_err(|e| format!("读取文件失败: {e}")))
        .await
        .map_err(|e| format!("任务执行失败: {e}"))?
}

#[tauri::command]
pub async fn write_text_file(dir: String, filename: String, text: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = PathBuf::from(dir.trim());
        fs::create_dir_all(&dir).map_err(|e| format!("创建目录失败: {e}"))?;
        let safe_name: String = filename
            .chars()
            .map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c })
            .collect();
        let path = dir.join(safe_name);
        fs::write(&path, text.as_bytes()).map_err(|e| format!("写入文件失败: {e}"))?;
        Ok(path.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| format!("任务执行失败: {e}"))?
}
