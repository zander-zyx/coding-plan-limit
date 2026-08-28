// 托盘悬停弹窗：48px 行 + hover 详情 + More 折叠 + 底部相对时间
// 头部仅状态点 + 齿轮；打开时后端已节流刷新
const list = document.getElementById('list');
const updatedEl = document.getElementById('updated');
const statusDot = document.getElementById('status-dot');

let moreOpen = false;
let lastViews = [];
let lastSettings = null;

function render(views, settings) {
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
  list.querySelector('#btn-more')?.addEventListener('click', () => {
    moreOpen = !moreOpen;
    render(lastViews, lastSettings);
  });

  // 底部相对时间
  const times = views.map((v) => v.snapshot && v.snapshot.updated_at).filter(Boolean);
  updatedEl.textContent = times.length ? `Updated ${fmtAgo(Math.max(...times))}` : '—';

  // 状态点：任一错误或紧急 → 琥珀脉冲；否则绿
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
    list.innerHTML = `<div class="p-empty">加载失败</div>`;
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

// ─── 更新按钮（仅在有新版本时出现在头部） ─────────────────────
let updateUrl = null;
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
listen('update-available', (e) => showUpdateBtn(e.payload));

// 鼠标进入/离开弹窗 → 上报后端参与隐藏判定
document.documentElement.addEventListener('mouseenter', () =>
  invoke('set_popup_hover', { active: true }));
document.documentElement.addEventListener('mouseleave', () =>
  invoke('set_popup_hover', { active: false }));
