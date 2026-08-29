// 更新下载前端接线：按钮点击 → 应用内下载（进度百分比显示）→ Windows 自动启动安装器
import fs from 'node:fs';

// ── http.rs 去掉未用导入 ──
{
  let s = fs.readFileSync('src-tauri/src/usage/http.rs', 'utf8');
  s = s.replace('use std::time::Duration;\nuse time::OffsetDateTime;', 'use std::time::Duration;');
  fs.writeFileSync('src-tauri/src/usage/http.rs', s);
  console.log('http.rs import cleaned');
}

// ── popup.js ──
{
  let s = fs.readFileSync('ui/js/popup.js', 'utf8');
  const before = s;

  s = s.replace(
    `let updateUrl = null;
const updateBtn = document.getElementById('btn-update');
function showUpdateBtn(info) {
  if (info && info.has_update && info.url) {
    updateUrl = info.url;
    updateBtn.hidden = false;
  }
}
updateBtn.addEventListener('click', () => {
  if (updateUrl) invoke('open_external', { url: updateUrl }).catch(() => {});
});
invoke('get_update_info').then(showUpdateBtn).catch(() => {});
listen('update-available', (e) => showUpdateBtn(e.payload));`,
    `let updateUrl = null;
let updateAsset = null;
let downloading = false;
const updateBtn = document.getElementById('btn-update');
const ICON_SVG = '<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3v12"/><path d="m7 10 5 5 5-5"/><path d="M4 19h16"/></svg>';

function showUpdateBtn(info) {
  if (info && info.has_update && info.url) {
    updateUrl = info.url;
    updateAsset = info.asset_url || null;
    updateBtn.hidden = false;
    updateBtn.innerHTML = ICON_SVG;
    updateBtn.disabled = false;
  }
}

async function startDownload() {
  if (downloading || !updateAsset) return;
  downloading = true;
  updateBtn.disabled = true;
  updateBtn.textContent = '0%';
  try {
    // Windows：下载完成后后端自动启动安装器并退出应用
    await invoke('download_and_install', { url: updateAsset });
  } catch (e) {
    toast(String(e));
    showUpdateBtn({ has_update: true, url: updateUrl, asset_url: updateAsset });
  } finally {
    downloading = false;
  }
}

listen('update-download-progress', (e) => {
  updateBtn.textContent = \`\${e.payload}%\`;
});

updateBtn.addEventListener('click', () => {
  if (downloading) return;
  if (updateAsset) startDownload();
  else if (updateUrl) invoke('open_external', { url: updateUrl }).catch(() => {});
});

invoke('get_update_info').then(showUpdateBtn).catch(() => {});
listen('update-available', (e) => showUpdateBtn(e.payload));`,
  );

  fs.writeFileSync('ui/js/popup.js', s);
  console.log('popup.js changed:', s !== before);
}

// ── app.js：侧边更新按钮同样走应用内下载 ──
{
  let a = fs.readFileSync('ui/js/app.js', 'utf8');
  const before = a;

  a = a.replace(
    `let updateUrl = null;
function showUpdateSide(info) {
  if (info && info.has_update && info.url) {
    updateUrl = info.url;
    $('btn-update-side').hidden = false;
  }
}
$('btn-update-side').addEventListener('click', () => {
  if (updateUrl) invoke('open_external', { url: updateUrl }).catch((e) => toast(String(e)));
});`,
    `let updateUrl = null;
let updateAsset = null;
let downloading = false;
const SIDE_SVG = '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3v12"/><path d="m7 10 5 5 5-5"/><path d="M4 19h16"/></svg>';

function showUpdateSide(info) {
  if (info && info.has_update && info.url) {
    updateUrl = info.url;
    updateAsset = info.asset_url || null;
    const b = $('btn-update-side');
    b.hidden = false;
    b.innerHTML = SIDE_SVG + '<i class="side-pulse"></i>';
    b.disabled = false;
  }
}

async function startSideDownload() {
  if (downloading || !updateAsset) return;
  downloading = true;
  const b = $('btn-update-side');
  b.disabled = true;
  b.textContent = '0%';
  try {
    await invoke('download_and_install', { url: updateAsset });
  } catch (e) {
    toast(String(e));
    showUpdateSide({ has_update: true, url: updateUrl, asset_url: updateAsset });
  } finally {
    downloading = false;
  }
}

listen('update-download-progress', (e) => {
  const b = $('btn-update-side');
  if (b && !b.hidden) b.textContent = \`\${e.payload}%\`;
});

$('btn-update-side').addEventListener('click', () => {
  if (downloading) return;
  if (updateAsset) startSideDownload();
  else if (updateUrl) invoke('open_external', { url: updateUrl }).catch((e) => toast(String(e)));
});`,
  );

  // 关于页"检查更新"命中新版本时优先直接下载
  a = a.replace(
    `    } else if (info.has_update) {
      showUpdateSide(info);
      invoke('open_external', { url: info.url }).catch(() => {});`,
    `    } else if (info.has_update) {
      showUpdateSide(info);
      toast(\`发现新版本 v\${info.latest}，点击侧边按钮直接更新\`);
      if (info.asset_url) {
        updateUrl = info.url;
        updateAsset = info.asset_url;
        $('btn-update-side').hidden = false;
        startSideDownload();
      }`,
  );

  fs.writeFileSync('ui/js/app.js', a);
  console.log('app.js changed:', a !== before);
}

// ── CSS：按钮文字态 ──
{
  let c = fs.readFileSync('ui/css/style.css', 'utf8');
  const before = c;
  c = c.replace(
    `.update-side {
  position: relative;
  width: 22px; height: 22px;`,
    `.update-side {
  position: relative;
  min-width: 22px; height: 22px;
  padding: 0 4px;
  font-size: 11px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;`,
  );
  c = c.replace(
    `.p-icon-btn.update-btn { opacity: 0.85; color: var(--brand); }`,
    `.p-icon-btn.update-btn { opacity: 0.85; color: var(--brand); min-width: 26px; font-size: 11px; font-weight: 600; font-variant-numeric: tabular-nums; }`,
  );
  fs.writeFileSync('ui/css/style.css', c);
  console.log('style.css changed:', c !== before);
}
