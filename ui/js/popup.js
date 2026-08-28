// 悬停弹窗逻辑：
//   固定展示区：设置中选择的 ≤2 家套餐（大卡片）
//   "更多"折叠区：其余套餐（紧凑行），点击展开/收起（记忆状态）
const list = document.getElementById('list');
const updatedEl = document.getElementById('updated');
const moreBtn = document.getElementById('btn-more');

let moreOpen = localStorage.getItem('cpl-more-open') === '1';
let lastViews = [];

function render(views) {
  lastViews = views;
  invoke('get_settings').then((s) => {
    const { primary, rest } = splitPopupViews(views, s.popup_plan_ids);

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
  });
}

(async () => {
  await initTheme();
  try {
    render(await invoke('get_views'));
  } catch (e) {
    list.innerHTML = `<div class="empty">加载失败<br />${esc(String(e))}</div>`;
  }

  await listen('views-updated', (e) => render(e.payload || []));
})();

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

// 鼠标进入/离开弹窗 → 上报后端，参与悬停状态判定（防止从托盘移过来时被误隐藏）
document.documentElement.addEventListener('mouseenter', () =>
  invoke('set_popup_hover', { active: true }));
document.documentElement.addEventListener('mouseleave', () =>
  invoke('set_popup_hover', { active: false }));

moreBtn.addEventListener('click', () => {
  moreOpen = !moreOpen;
  localStorage.setItem('cpl-more-open', moreOpen ? '1' : '0');
  render(lastViews);
});
