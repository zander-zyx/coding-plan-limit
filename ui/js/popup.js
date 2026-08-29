// 托盘悬停弹窗：48px 行 + hover 详情 + More 折叠 + 底部相对时间
// 头部：状态点（点击关闭）+ 齿轮；打开时后端已节流刷新
const list = document.getElementById('list');
const updatedEl = document.getElementById('updated');
const statusDot = document.getElementById('status-dot');

let moreOpen = false;
let lastViews = [];
let lastSettings = null;

// 弹窗高度跟随内容：收起时按显示行数自适应、展开"更多"后拉长；
// setSize 后请后端按托盘位置重新定位（保持底部锚定），超出上限则列表内部滚动
async function fitWindow() {
  try {
    const panel = document.querySelector('.panel');
    panel.style.height = 'auto';
    const h = Math.ceil(document.body.scrollHeight);
    panel.style.height = '';
    const capped = Math.min(Math.max(h, 120), 680);
    await window.__TAURI__.window.getCurrentWindow()
      .setSize(new window.__TAURI__.dpi.LogicalSize(280, capped));
    invoke('popup_size_changed').catch(() => {});
  } catch { /* 无窗口 API 的环境（如浏览器预览）静默 */ }
}

function render(views, settings) {
  try {
    lastViews = views;
    const s = settings || lastSettings;
    if (!s) return;
    lastSettings = s;

    const { primary, rest } = splitPopupViews(views, s.popup_plan_ids);
    const enabled = views.filter((v) => v.plan.enabled);

    if (!primary.length) {
      list.innerHTML = `<div class="p-empty">暂无启用的套餐</div>`;
      updatedEl.textContent = '—';
      statusDot.className = 'status-dot ok';
      fitWindow();
      return;
    }

    let html = primary.map((v) => rowHtml(v)).join('');
    if (rest.length) {
      html += `
        <button class="more-toggle" id="btn-more">
          <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" class="${moreOpen ? 'up' : ''}"><path d="m6 9 6 6 6-6"/></svg>
          更多 ${rest.length}
        </button>`;
      if (moreOpen) {
        html += `<div class="more-wrap">${rest.map((v) => rowHtml(v, { sub: true })).join('')}</div>`;
      }
    }
    list.innerHTML = html;
    animateBars(list);
    list.querySelector('#btn-more')?.addEventListener('click', () => {
      moreOpen = !moreOpen;
      render(lastViews, lastSettings);
    });

    const times = views.map((v) => v.snapshot && v.snapshot.updated_at).filter(Boolean);
    updatedEl.textContent = times.length ? `Updated ${fmtAgo(Math.max(...times))}` : '—';

    const bad = enabled.some((v) => v.snapshot && !v.snapshot.ok)
      || enabled.some((v) => {
        const q = v.snapshot?.quota;
        if (!q) return false;
        if (q.kind === 'windows') return q.windows.some((w) => isUrgent(w.used_percent));
        if (q.kind === 'fixed_quota') return isUrgent(q.used_percent);
        if (q.kind === 'balance') return q.amount <= v.plan.threshold;
        return false;
      });
    statusDot.className = `status-dot ${bad ? 'warn' : 'ok'}`;
    fitWindow();
  } catch (e) {
    list.innerHTML = `<div class="p-empty" style="color:var(--urgent)">渲染异常：${esc(String(e && e.message || e))}</div>`;
    console.error('popup render failed:', e);
  }
}

async function loadSettings() {
  try {
    lastSettings = await invoke('get_settings');
    applySettingsLook(lastSettings);
  } catch { /* 保持现状 */ }
}

(async () => {
  await loadSettings();
  try {
    render(await invoke('get_views'), lastSettings);
  } catch (e) {
    list.innerHTML = `<div class="p-empty" style="color:var(--urgent)">加载失败：${esc(String(e && e.message || e))}</div>`;
    console.error('popup load failed:', e);
  }

  await listen('views-updated', async (e) => {
    await loadSettings();
    render(e.payload || [], lastSettings);
  });
  await listen('settings-updated', async (e) => {
    lastSettings = e.payload || lastSettings;
    applySettingsLook(lastSettings);
    render(lastViews, lastSettings);
  });
})();

// 点击行 → 跳转该套餐官网（无官网的打开主界面）
list.addEventListener('click', (e) => {
  if (e.target.closest('#btn-more')) return;
  const row = e.target.closest('.row');
  if (!row) return;
  const meta = PROVIDER_META[row.dataset.template];
  if (meta && meta.homepage) {
    invoke('open_external', { url: meta.homepage }).catch(() => {});
  } else {
    invoke('open_main');
  }
});

document.getElementById('btn-settings').addEventListener('click', () => invoke('open_main'));

// 状态点：点击关闭弹窗（Windows 悬停驱动，重入托盘会再次打开）
statusDot.addEventListener('click', () => invoke('hide_popup').catch(() => {}));

// ─── 更新按钮（仅在有新版本时出现在头部） ─────────────────────
let updateUrl = null;
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
  updateBtn.textContent = `${e.payload}%`;
});

updateBtn.addEventListener('click', () => {
  if (downloading) return;
  if (updateAsset) startDownload();
  else if (updateUrl) invoke('open_external', { url: updateUrl }).catch(() => {});
});

invoke('get_update_info').then(showUpdateBtn).catch(() => {});
listen('update-available', (e) => showUpdateBtn(e.payload));

// 鼠标进入/离开弹窗 → 上报后端参与隐藏判定
document.documentElement.addEventListener('mouseenter', () =>
  invoke('set_popup_hover', { active: true }));
document.documentElement.addEventListener('mouseleave', () =>
  invoke('set_popup_hover', { active: false }));
