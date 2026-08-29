// 更新功能升级：UpdateInfo 带平台安装包直链 + 应用内下载并启动安装器
import fs from 'node:fs';

// ── update.rs ──
{
  let s = fs.readFileSync('src-tauri/src/update.rs', 'utf8');
  const before = s;

  // UpdateInfo 增加 asset_url
  s = s.replace(
    `pub struct UpdateInfo {
    pub has_update: bool,
    pub current: String,
    pub latest: String,
    pub url: String,`,
    `pub struct UpdateInfo {
    pub has_update: bool,
    pub current: String,
    pub latest: String,
    pub url: String,
    /// 当前平台安装包直链（无匹配产物时为 None，前端回退打开 Releases 页）
    pub asset_url: Option<String>,`,
  );

  // base 初始化
  s = s.replace(
    `        url: RELEASES_PAGE.to_string(),
        error: None,
    };`,
    `        url: RELEASES_PAGE.to_string(),
        asset_url: None,
        error: None,
    };`,
  );

  // 解析 assets 选平台安装包直链
  s = s.replace(
    `                    let url = json
                        .get("html_url")
                        .and_then(|v| v.as_str())
                        .unwrap_or(RELEASES_PAGE)
                        .to_string();`,
    `                    let url = json
                        .get("html_url")
                        .and_then(|v| v.as_str())
                        .unwrap_or(RELEASES_PAGE)
                        .to_string();
                    let asset_url = json
                        .get("assets")
                        .and_then(|v| v.as_array())
                        .and_then(|assets| {
                            assets
                                .iter()
                                .filter_map(|a| {
                                    a.get("browser_download_url").and_then(|u| u.as_str())
                                })
                                .find(|u| asset_matches(u))
                                .map(String::from)
                        });`,
  );

  // 组装处带 asset_url
  s = s.replace(
    `UpdateInfo { has_update, latest, url, ..base }`,
    `UpdateInfo { has_update, latest, url, asset_url, ..base }`,
  );

  // 平台产物匹配 + 下载命令
  s += `

/// 按当前平台挑选安装包资产名（GitHub Release asset）
fn asset_matches(url: &str) -> bool {
    let u = url.to_lowercase();
    #[cfg(target_os = "windows")]
    {
        u.ends_with("x64-setup.exe")
    }
    #[cfg(target_os = "macos")]
    {
        #[cfg(target_arch = "aarch64")]
        {
            u.ends_with("aarch64.dmg")
        }
        #[cfg(not(target_arch = "aarch64"))]
        {
            u.ends_with("x64.dmg")
        }
    }
    #[cfg(target_os = "linux")]
    {
        u.ends_with("amd64.appimage")
    }
}

/// 应用内直接下载当前平台安装包（进度经 update-download-progress 事件推送），
/// Windows 下载完成后自动启动安装器并退出应用；macOS/Linux 打开所在文件夹。
#[tauri::command]
pub async fn download_and_install(app: AppHandle, url: String) -> Result<(), String> {
    use std::io::Write;
    use tauri::Emitter;

    if !asset_matches(&url) {
        return Err("该链接不是当前平台的安装包".into());
    }
    let fname = url
        .split('/')
        .next_back()
        .unwrap_or("plan-limit-setup")
        .split('?')
        .next()
        .unwrap_or("plan-limit-setup")
        .to_string();
    let dir = app
        .path()
        .download_dir()
        .map_err(|e| format!("无法定位下载目录: {e}"))?;
    let dest = dir.join(&fname);

    let mut resp = http::client()
        .get(&url)
        .timeout(std::time::Duration::from_secs(600))
        .send()
        .await
        .map_err(|e| format!("下载失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("下载失败: HTTP {}", resp.status().as_u16()));
    }
    let total = resp.content_length().unwrap_or(0);
    let mut file = std::fs::File::create(&dest).map_err(|e| format!("创建文件失败: {e}"))?;
    let mut downloaded: u64 = 0;
    let mut last_pct: i64 = -1;
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| format!("下载中断: {e}"))?
    {
        file.write_all(&chunk).map_err(|e| format!("写入失败: {e}"))?;
        downloaded += chunk.len() as u64;
        if total > 0 {
            let pct = (downloaded * 100 / total) as i64;
            if pct != last_pct {
                last_pct = pct;
                let _ = app.emit("update-download-progress", pct);
            }
        }
    }
    file.flush().ok();
    drop(file);

    #[cfg(target_os = "windows")]
    {
        // 启动 NSIS 安装器（带向导，可改目录），随后退出本应用
        std::process::Command::new(&dest)
            .spawn()
            .map_err(|e| format!("启动安装器失败: {e}"))?;
        app.exit(0);
    }

    // macOS/Linux：无法静默安装，打开所在文件夹引导用户
    #[cfg(not(target_os = "windows"))]
    {
        use tauri_plugin_opener::OpenerExt;
        if let Some(folder) = dest.parent() {
            let _ = app.opener().open_path(folder.to_string_lossy(), None::<&str>);
        }
    }
    Ok(())
}
`;
  fs.writeFileSync('src-tauri/src/update.rs', s);
  console.log('update.rs changed:', s !== before);
}

// ── main.rs：注册命令 + PathResolver 导入 ──
{
  let m = fs.readFileSync('src-tauri/src/main.rs', 'utf8');
  const before = m;
  m = m.replace(
    `            update::check_update,
            update::get_update_info,`,
    `            update::check_update,
            update::get_update_info,
            update::download_and_install,`,
  );
  fs.writeFileSync('src-tauri/src/main.rs', m);
  console.log('main.rs changed:', m !== before);
}
