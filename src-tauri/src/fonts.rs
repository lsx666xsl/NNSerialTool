/*
*   描述：系统字体枚举（设置界面字体下拉的数据源）
*   实现：font-kit 走 DirectWrite 系统字体集合，返回的是 CSS 可直接使用的字体家族名
*         （而非字体文件名），覆盖系统预装 + 用户自行安装的字体
*/
#[tauri::command]
pub async fn system_fonts_list() -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let mut families = font_kit::source::SystemSource::new()
            .all_families()
            .map_err(|e| format!("枚举系统字体失败: {}", e))?;

        // 过滤空名，按忽略大小写的字母序去重排序（下拉展示稳定）
        families.retain(|f| !f.trim().is_empty());
        families.sort_by_key(|f| f.to_lowercase());
        families.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        Ok(families)
    })
    .await
    .map_err(|e| format!("任务执行失败: {}", e))?
}
