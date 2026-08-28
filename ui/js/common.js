// ============================================================
// Coding Plan Limit — 前端共享库（无框架，依赖 Tauri withGlobalTauri）
// ============================================================

const Tauri = window.__TAURI__;
const invoke = Tauri.core.invoke;
const listen = Tauri.event.listen;

// ─── 模板元数据 ────────────────────────────────────────────────
const PROVIDER_META = {
  minimax:      { name: 'MiniMax',       color: '#ff5b4a' },
  zhipu:        { name: '智谱 GLM',      color: '#3f7cff' },
  'kimi-coding':{ name: 'Kimi Coding',   color: '#16c8b7' },
  'claude-cache':{ name: 'Claude',       color: '#d97757' },
  xiaomi:       { name: '小米 MiMo',     color: '#ff6900' },
  deepseek:     { name: 'DeepSeek',      color: '#4d6bfe' },
  kimi:         { name: 'Kimi',          color: '#0ea5a3' },
  stepfun:      { name: '阶跃星辰',      color: '#8b5cf6' },
  siliconflow:  { name: '硅基流动',      color: '#6366f1' },
  alibaba:      { name: '阿里云',        color: '#f59e0b' },
};

// ─── 主题 ─────────────────────────────────────────────────────
let _systemDark = window.matchMedia('(prefers-color-scheme: dark)');

function applyTheme(pref) {
  const dark = pref === 'dark' || (pref !== 'light' && _systemDark.matches);
  document.documentElement.dataset.theme = dark ? 'dark' : 'light';
}

_systemDark.addEventListener('change', () => {
  if (window.__themePref && window.__themePref !== 'light' && window.__themePref !== 'dark') {
    applyTheme('system');
  }
});

async function initTheme() {
  try {
    const s = await invoke('get_settings');
    window.__themePref = s.theme || 'system';
    applyAccent(s.accent);
  } catch {
    window.__themePref = 'system';
  }
  applyTheme(window.__themePref);
}

/** 应用自定义主题色（#RRGGBB），并按亮度决定高亮文字用深/浅 */
function applyAccent(hex) {
  if (!hex || !/^#[0-9a-fA-F]{6}$/.test(hex)) return;
  const root = document.documentElement;
  root.style.setProperty('--accent', hex);
  const n = parseInt(hex.slice(1), 16);
  const lum = 0.2126 * (n >> 16) + 0.7152 * ((n >> 8) & 0xff) + 0.0722 * (n & 0xff);
  root.style.setProperty('--accent-contrast', lum > 165 ? '#1d2231' : '#ffffff');
}

// ─── 工具 ─────────────────────────────────────────────────────
function esc(s) {
  return String(s ?? '').replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));
}

function fmtClock(unixSecs) {
  if (!unixSecs) return '—';
  return new Date(unixSecs * 1000).toLocaleTimeString('zh-CN', { hour12: false });
}

function fmtCountdown(unixSecs) {
  if (!unixSecs) return '';
  const diff = unixSecs - Date.now() / 1000;
  if (diff <= 0) return '即将重置';
  const h = Math.floor(diff / 3600);
  const m = Math.floor((diff % 3600) / 60);
  return h > 0 ? `${h}小时${m}分后重置` : `${Math.max(m, 1)}分钟后重置`;
}

/** 大数字缩写：12345 → 1.2万 */
function fmtNum(n) {
  if (n >= 1e8) return (n / 1e8).toFixed(1) + '亿';
  if (n >= 1e4) return (n / 1e4).toFixed(1) + '万';
  return String(Math.round(n));
}

/** 剩余百分比 → 状态类名 */
function statusClass(remainingPct) {
  if (remainingPct <= 10) return 'st-bad';
  if (remainingPct <= 25) return 'st-warn';
  return 'st-ok';
}

// ─── SVG 片段 ─────────────────────────────────────────────────
const RING_R = 19;
const RING_C = 2 * Math.PI * RING_R;

/**
 * 环形百分比（剩余量），首次从 0 平滑过渡到目标值
 * @param size 直径 px
 */
function ringHtml(remainingPct, size, label) {
  const frac = Math.min(Math.max(remainingPct, 0), 100) / 100;
  const offset = (RING_C * (1 - frac)).toFixed(2);
  return `
  <div class="ring-wrap" style="width:${size}px;height:${size}px">
    <svg class="ring" width="${size}" height="${size}" viewBox="0 0 44 44">
      <circle class="ring-bg" cx="22" cy="22" r="${RING_R}"/>
      <circle class="ring-fg" cx="22" cy="22" r="${RING_R}"
        stroke-dasharray="${RING_C.toFixed(2)}" stroke-dashoffset="${RING_C.toFixed(2)}"
        data-target="${offset}"/>
    </svg>
    <span class="ring-label" style="font-size:${size >= 56 ? 12 : 10.5}px">${esc(label)}</span>
  </div>`;
}

/** 渲染后触发环形/进度条从 0 → 目标值的平滑动画 */
function animateCards(container) {
  requestAnimationFrame(() => requestAnimationFrame(() => {
    container.querySelectorAll('.ring-fg[data-target]').forEach((el) => {
      el.style.strokeDashoffset = el.dataset.target;
      el.removeAttribute('data-target');
    });
    container.querySelectorAll('.bar > i[data-w]').forEach((el) => {
      el.style.width = el.dataset.w + '%';
      el.removeAttribute('data-w');
    });
  }));
}

// ─── 卡片渲染（弹窗与仪表盘共用） ─────────────────────────────
function metaOf(view) {
  return PROVIDER_META[view.plan.template] || { name: view.plan.template, color: '#8b93a7' };
}

/** 单个窗口区块：标签+重置 / 大号已用百分比 / 通栏平滑进度条 */
function winBlockHtml(w, compact) {
  const used = w.used_percent;
  const st = used >= 90 ? 'st-bad' : used >= 75 ? 'st-warn' : 'st-ok';
  const reset = fmtCountdown(w.reset_at);
  return `
  <div class="win-block ${st}">
    <div class="win-head">
      <span>${esc(w.label)}</span>
      ${reset ? `<span class="reset">${esc(reset)}</span>` : ''}
    </div>
    <div class="win-pct ${compact ? 'compact' : ''}">
      <b>${used.toFixed(used < 10 ? 1 : 0)}</b><span class="u">% 已使用</span>
    </div>
    <div class="bar ${st}"><i data-w="${used.toFixed(1)}"></i></div>
  </div>`;
}

/** 多窗口收进紧凑行（弹窗"更多"区） */
function winRowsHtml(windows) {
  const rows = windows.map((w) => {
    const used = w.used_percent;
    const st = used >= 90 ? 'st-bad' : used >= 75 ? 'st-warn' : 'st-ok';
    return `
    <div class="win-row">
      <span>${esc(w.label)}</span>
      <div class="bar slim ${st}"><i data-w="${used.toFixed(1)}"></i></div>
      <b>${(100 - used).toFixed(0)}%</b>
    </div>`;
  }).join('');
  return `<div class="win-rows">${rows}</div>`;
}

/**
 * 渲染单个套餐卡片
 * @param mode 'popup' 弹窗主卡片 | 'mini' 弹窗"更多"区 | 'dash' 仪表盘
 */
function cardHtml(view, mode) {
  const m = metaOf(view);
  const name = esc(view.plan.name);
  const badge = `<span class="tpl-badge" style="color:${m.color}">${esc(m.name)}</span>`;
  const compact = mode !== 'dash';

  let body = '';
  let ringPct = null;

  const snap = view.snapshot;
  if (!snap || !snap.ok) {
    const err = snap && snap.error ? esc(snap.error) : '尚未获取数据';
    body = `<div class="error">⚠ ${err}</div>`;
  } else {
    const q = snap.quota;
    if (q.kind === 'windows') {
      const wins = q.windows;
      if (mode === 'mini') {
        body = winRowsHtml(wins);
      } else {
        // 主窗口大块展示，其余窗口收进行
        body = winBlockHtml(wins[0], compact);
        if (wins.length > 1) body += winRowsHtml(wins.slice(1));
      }
      const worst = wins.reduce(
        (acc, w) => (100 - w.used_percent < acc ? 100 - w.used_percent : acc),
        100,
      );
      ringPct = worst;
    } else if (q.kind === 'balance') {
      body = `
        <div class="big-amount"><small>${esc(q.currency || 'CNY')}</small> ${q.amount.toFixed(2)}</div>
        ${q.note ? `<div class="card-foot">${esc(q.note)}</div>` : ''}`;
    } else if (q.kind === 'fixed_quota') {
      if (mode === 'mini') {
        body = `
        <div class="win-rows">
          <div class="win-row"><span>已用</span>
            <div class="bar slim ${100 - q.used_percent <= 10 ? 'st-bad' : 100 - q.used_percent <= 25 ? 'st-warn' : 'st-ok'}"><i data-w="${q.used_percent.toFixed(1)}"></i></div>
            <b>${(100 - q.used_percent).toFixed(0)}%</b>
          </div>
        </div>`;
      } else {
        body = winBlockHtml(
          { label: '总额度', used_percent: q.used_percent, reset_at: q.reset_at },
          compact,
        ) + `<div class="card-foot">已用 ${fmtNum(q.used)} / 共 ${fmtNum(q.total)} ${esc(q.unit || '')}</div>`;
      }
      ringPct = 100 - q.used_percent;
    }
  }

  const ring = mode === 'dash' && ringPct !== null
    ? ringHtml(ringPct, 58, `${Math.round(ringPct)}%`)
    : '';
  const cls = ringPct !== null ? statusClass(ringPct) : '';
  const updated = mode === 'dash' && snap
    ? `<div class="card-foot">更新于 ${fmtClock(snap.updated_at)}</div>`
    : '';

  const inner = mode === 'dash'
    ? `<div class="card-top"><div class="card-main">
         <div class="card-title">${name}${badge}</div>
         ${body}${updated}
       </div>${ring}</div>`
    : `<div class="card-main">
         <div class="card-title">${name}${badge}</div>
         ${body}
       </div>`;

  return `<div class="card ${cls} ${mode === 'dash' ? 'dash-card' : ''}">${inner}</div>`;
}

// ─── 弹窗视图拆分：固定展示 ≤2 家，其余收进"更多" ────────────
function splitPopupViews(views, popupPlanIds) {
  const enabled = views.filter((v) => v.plan.enabled);
  const byId = new Map(enabled.map((v) => [v.plan.id, v]));
  const picked = (popupPlanIds || [])
    .map((id) => byId.get(id))
    .filter(Boolean)
    .slice(0, 2);
  for (const v of enabled) {
    if (picked.length >= 2) break;
    if (!picked.includes(v)) picked.push(v);
  }
  const rest = enabled.filter((v) => !picked.includes(v));
  return { primary: picked, rest };
}

// ─── Toast ────────────────────────────────────────────────────
let _toastTimer = null;
function toast(msg, warn) {
  let el = document.getElementById('toast');
  if (!el) {
    el = document.createElement('div');
    el.id = 'toast';
    document.body.appendChild(el);
  }
  el.textContent = msg;
  el.classList.toggle('warn', !!warn);
  el.classList.add('show');
  clearTimeout(_toastTimer);
  _toastTimer = setTimeout(() => el.classList.remove('show'), 3200);
}
