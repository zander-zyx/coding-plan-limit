// 悬停弹窗逻辑：
//   固定展示区：设置中选择的 ≤10 家套餐（大卡片，点击卡片跳转官网）
//   "更多"折叠区：其余套餐（紧凑行），点击展开/收起（记忆状态）
//   打开时后端已强制刷新一轮；设置变更（主题色/进度样式）实时跟随
const list = document.getElementById('list');
const updatedEl = document.getElementById('updated');
const moreBtn = document.getElementById('btn-more');

let moreOpen = localStorage.getItem('cpl-more-open') === '1';
let lastViews = [];
let lastSettings = null;

function render(views, settings) {
  lastViews = views;
  const s = settings || lastSettings;
  if (!s) return;
  lastSettings = s;

  const { primary, rest } = splitPopupViews(views, s.popup_plan_ids, 10);

  if (!primary.length) {
    list.innerHTML = `
      <div class="empty">
        还没有启用的套餐<br />
        点击右下角「设置」添加
      </div>`;
    moreBtn.hidden = true;
    updatedEl.textContent = '—';
    return;
  }

  let html = primary.map((v) => cardHtml(v, 'popup')).join('');

  if (rest.length) {
    moreBtn.hidden = false;
    moreBtn.textContent = moreOpen ? `收起（${rest.length}）` : `更多（${rest.length}）`;
    if (moreOpen) {
      html += `
        <div class="more-divider">更多套餐</div>
        <div class="more-list">${rest.map((v) => cardHtml(v, 'mini')).join('')}</div>`;
    }
  } else {
    moreBtn.hidden = true;
  }

  list.innerHTML = html;
  animateCards(list);

  const times = views
    .map((v) => v.snapshot && v.snapshot.updated_at)
    .filter(Boolean);
  updatedEl.textContent = times.length
    ? `更新于 ${fmtClock(Math.max(...times))}`
    : '—';
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
    list.innerHTML = `<div class="empty">加载失败<br />${esc(String(e))}</div>`;
  }

  await listen('views-updated', (e) => render(e.payload || [], lastSettings));
  // 设置变更（主题色/主题/进度样式/固定展示）实时跟随
  await listen('settings-updated', async (e) => {
    lastSettings = e.payload || lastSettings;
    applySettingsLook(lastSettings);
    render(lastViews, lastSettings);
  });
})();

// 点击卡片 → 跳转该套餐官网（空白首页的模板跳主界面）
list.addEventListener('click', (e) => {
  const card = e.target.closest('.card');
  if (!card) return;
  const meta = PROVIDER_META[card.dataset.template];
  if (meta && meta.homepage) {
    invoke('open_external', { url: meta.homepage }).catch((err) => toast(String(err), true));
  } else {
    invoke('open_main');
  }
});

document.getElementById('btn-refresh').addEventListener('click', () => {
  document.getElementById('btn-refresh').classList.add('spinning');
  invoke('refresh_now');
  setTimeout(
    () => document.getElementById('btn-refresh')?.classList.remove('spinning'),
    1200,
  );
});
document.getElementById('btn-open').addEventListener('click', () => invoke('open_main'));
document.getElementById('btn-settings').addEventListener('click', () => invoke('open_main'));

// ─── 更新按钮：有新版本时出现在标题栏，点击直达下载页 ──────────
let updateUrl = null;
const updateBtn = document.getElementById('btn-update');
function showUpdateBtn(info) {
  if (info && info.has_update && info.url) {
    updateUrl = info.url;
    updateBtn.hidden = false;
  }
}
updateBtn.addEventListener('click', () => {
  if (updateUrl) invoke('open_external', { url: updateUrl }).catch((e) => toast(String(e), true));
});
invoke('get_update_info').then(showUpdateBtn).catch(() => {});
listen('update-available', (e) => showUpdateBtn(e.payload));

moreBtn.addEventListener('click', () => {
  moreOpen = !moreOpen;
  localStorage.setItem('cpl-more-open', moreOpen ? '1' : '0');
  render(lastViews, lastSettings);
});
