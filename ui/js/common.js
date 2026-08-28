// ============================================================
// Coding Plan Limit — 前端共享库（无框架，依赖 Tauri withGlobalTauri）
// 所有百分比统一展示"已使用"；进度样式 bar（平滑填充，默认）/ ring（环形）
// ============================================================

const Tauri = window.__TAURI__;
const invoke = Tauri.core.invoke;
const listen = Tauri.event.listen;

// ─── 模板元数据（name/color/icon/homepage 与后端 templates() 对应） ──
const PROVIDER_META = {
  minimax:        { name: 'MiniMax',       color: '#ff5b4a', icon: 'icons/minimax.svg', homepage: 'https://platform.minimaxi.com' },
  zhipu:          { name: '智谱 GLM',      color: '#3f7cff', icon: 'icons/zhipu.svg', homepage: 'https://open.bigmodel.cn' },
  'kimi-coding':  { name: 'Kimi Coding',   color: '#16c8b7', icon: 'icons/kimi.svg', homepage: 'https://www.kimi.com/coding' },
  'claude-official': { name: 'Claude',     color: '#d97757', icon: 'icons/claude-official.svg', homepage: 'https://claude.ai' },
  codex:          { name: 'Codex',         color: '#10a37f', icon: 'icons/codex.svg', homepage: 'https://chatgpt.com' },
  'claude-cache': { name: 'Claude',        color: '#d97757', icon: 'icons/claude-cache.svg', homepage: 'https://claude.ai' },
  xiaomi:         { name: '小米 MiMo',     color: '#ff6900', icon: 'icons/xiaomi.svg', homepage: 'https://platform.xiaomimimo.com' },
  deepseek:       { name: 'DeepSeek',      color: '#4d6bfe', icon: 'icons/deepseek.svg', homepage: 'https://platform.deepseek.com' },
  kimi:           { name: 'Kimi',          color: '#0ea5a3', icon: 'icons/kimi.svg', homepage: 'https://platform.moonshot.cn' },
  stepfun:        { name: '阶跃星辰',      color: '#8b5cf6', icon: 'icons/stepfun.svg', homepage: 'https://platform.stepfun.com' },
  siliconflow:    { name: '硅基流动',      color: '#6366f1', icon: 'icons/siliconflow.svg', homepage: 'https://cloud.siliconflow.cn' },
  alibaba:        { name: '阿里云',        color: '#f59e0b', icon: 'icons/alibaba.svg', homepage: 'https://bailian.console.aliyun.com' },
  packycode:      { name: 'PackyCode',     color: '#7c5cff', icon: 'icons/packycode.svg', homepage: 'https://www.packyapi.ai' },
  newapi:         { name: 'NewAPI',        color: '#38bdf8', icon: '', homepage: '' },
  sub2api:        { name: 'Sub2API',       color: '#94a3b8', icon: '', homepage: '' },
};

// ─── 主题 / 设置 ───────────────────────────────────────────────
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

/** 应用自定义主题色（#RRGGBB），并按亮度决定高亮文字用深/浅 */
function applyAccent(hex) {
  if (!hex || !/^#[0-9a-fA-F]{6}$/.test(hex)) return;
  const root = document.documentElement;
  root.style.setProperty('--accent', hex);
  const n = parseInt(hex.slice(1), 16);
  const lum = 0.2126 * (n >> 16) + 0.7152 * ((n >> 8) & 0xff) + 0.0722 * (n & 0xff);
  root.style.setProperty('--accent-contrast', lum > 165 ? '#1d2231' : '#ffffff');
}

/** 把设置对象落到全局外观状态（主题 / 主题色 / 进度样式） */
function applySettingsLook(s) {
  if (!s) return;
  window.__themePref = s.theme || 'system';
  applyTheme(window.__themePref);
  applyAccent(s.accent);
  window.__barStyle = s.bar_style === 'ring' ? 'ring' : 'bar';
}

async function initTheme() {
  try {
    applySettingsLook(await invoke('get_settings'));
  } catch {
    window.__themePref = 'system';
    window.__barStyle = 'bar';
  }
  applyTheme(window.__themePref);
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

/** 已用百分比 → 状态类名（颜色随用量升高变黄变红） */
function usedStatusCls(usedPct) {
  if (usedPct >= 90) return 'st-bad';
  if (usedPct >= 75) return 'st-warn';
  return 'st-ok';
}

// ─── SVG 环形 ─────────────────────────────────────────────────
const RING_R = 19;
const RING_C = 2 * Math.PI * RING_R;

/**
 * 环形（已用百分比），从 0 平滑过渡到目标值
 * @param cls 状态类（st-ok/st-warn/st-bad）控制颜色
 */
function ringHtml(usedPct, size, label, cls) {
  const frac = Math.min(Math.max(usedPct, 0), 100) / 100;
  const offset = (RING_C * (1 - frac)).toFixed(2);
  return `
  <div class="ring-wrap ${cls}" style="width:${size}px;height:${size}px">
    <svg class="ring" width="${size}" height="${size}" viewBox="0 0 44 44">
      <circle class="ring-bg" cx="22" cy="22" r="${RING_R}"/>
      <circle class="ring-fg" cx="22" cy="22" r="${RING_R}"
        stroke-dasharray="${RING_C.toFixed(2)}" stroke-dashoffset="${RING_C.toFixed(2)}"
        data-target="${offset}"/>
    </svg>
    <span class="ring-label" style="font-size:${size >= 56 ? 11.5 : 10}px">${esc(label)}</span>
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

// ─── 卡片片段 ─────────────────────────────────────────────────
function metaOf(view) {
  return PROVIDER_META[view.plan.template] || { name: view.plan.template, color: '#8b93a7', icon: '', homepage: '' };
}

/** 套餐 logo：自定义优先，否则用内置品牌图标；都没有则显示品牌色菱形 */
function logoHtml(view) {
  const m = metaOf(view);
  const src = view.plan.logo || m.icon;
  if (src) {
    return `<img class="card-logo" src="${esc(src)}" alt="" onerror="this.classList.add('logo-broken')" />`;
  }
  return `<i class="tpl-glyph" style="background:${m.color}"></i>`;
}

/** 单窗口块：标签+重置 / 大号已用% / 通栏平滑进度条 */
function winBlockHtml(w, compact) {
  const used = w.used_percent;
  const cls = usedStatusCls(used);
  const reset = fmtCountdown(w.reset_at);
  return `
  <div class="win-block ${cls}">
    <div class="win-head">
      <span>${esc(w.label)}</span>
      ${reset ? `<span class="reset">${esc(reset)}</span>` : ''}
    </div>
    <div class="win-pct ${compact ? 'compact' : ''}">
      <b>${used.toFixed(used < 10 ? 1 : 0)}</b><span class="u">% 已使用</span>
    </div>
    <div class="bar ${cls}"><i data-w="${used.toFixed(1)}"></i></div>
  </div>`;
}

/** 多窗口紧凑行（统一显示已用%） */
function winRowsHtml(windows) {
  const rows = windows.map((w) => {
    const cls = usedStatusCls(w.used_percent);
    return `
    <div class="win-row">
      <span>${esc(w.label)}</span>
      <div class="bar slim ${cls}"><i data-w="${w.used_percent.toFixed(1)}"></i></div>
      <b>${w.used_percent.toFixed(w.used_percent < 10 ? 1 : 0)}%</b>
    </div>`;
  }).join('');
  return `<div class="win-rows">${rows}</div>`;
}

/** 环形样式：每个窗口一枚环形（已用%） */
function ringRowHtml(windows, compact) {
  const items = windows.map((w) => {
    const cls = usedStatusCls(w.used_percent);
    const reset = w.reset_at ? `<span class="ring-reset">${esc(fmtCountdown(w.reset_at))}</span>` : '';
    return `
    <div class="ring-item">
      ${ringHtml(w.used_percent, compact ? 50 : 60, `${w.used_percent.toFixed(0)}%`, cls)}
      <span class="ring-cap">${esc(w.label)}</span>
      ${reset}
    </div>`;
  }).join('');
  return `<div class="ring-row">${items}</div>`;
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
  const style = window.__barStyle || 'bar';

  let body = '';
  let worstUsed = null;

  const snap = view.snapshot;
  if (!snap || !snap.ok) {
    const err = snap && snap.error ? String(snap.error) : '尚未获取数据';
    body = err.startsWith('暂不支持')
      ? `<div class="notice">ℹ 该套餐暂无窗口额度数据</div>`
      : `<div class="error">⚠ ${esc(err)}</div>`;
  } else {
    const q = snap.quota;
    if (q.kind === 'windows') {
      const wins = q.windows;
      if (mode === 'mini') {
        body = style === 'ring' ? ringRowHtml(wins, true) : winRowsHtml(wins);
      } else if (style === 'ring') {
        body = ringRowHtml(wins, compact);
      } else {
        body = winBlockHtml(wins[0], compact);
        if (wins.length > 1) body += winRowsHtml(wins.slice(1));
      }
      worstUsed = wins.reduce((a, w) => Math.max(a, w.used_percent), 0);
    } else if (q.kind === 'balance') {
      body = `
        <div class="big-amount"><small>${esc(q.currency || 'CNY')}</small> ${q.amount.toFixed(2)}</div>
        ${q.note ? `<div class="card-foot">${esc(q.note)}</div>` : ''}`;
    } else if (q.kind === 'fixed_quota') {
      if (mode === 'mini') {
        body = `<div class="win-rows">
          <div class="win-row"><span>总额度</span>
            <div class="bar slim ${usedStatusCls(q.used_percent)}"><i data-w="${q.used_percent.toFixed(1)}"></i></div>
            <b>${q.used_percent.toFixed(0)}%</b>
          </div>
        </div>`;
      } else if (style === 'ring') {
        body = ringRowHtml(
          [{ label: '总额度', used_percent: q.used_percent, reset_at: q.reset_at }],
          compact,
        );
      } else {
        body = winBlockHtml(
          { label: '总额度', used_percent: q.used_percent, reset_at: q.reset_at },
          compact,
        );
      }
      if (mode !== 'mini') {
        body += `<div class="card-foot">已用 ${fmtNum(q.used)} / 共 ${fmtNum(q.total)} ${esc(q.unit || '')}</div>`;
      }
      worstUsed = q.used_percent;
    }
  }

  const cls = worstUsed !== null ? usedStatusCls(worstUsed) : '';
  const updated = mode === 'dash' && snap
    ? `<div class="card-foot">更新于 ${fmtClock(snap.updated_at)}</div>`
    : '';

  const inner = mode === 'dash'
    ? `<div class="card-top"><div class="card-main">
         <div class="card-title">${logoHtml(view)}${name}${badge}</div>
         ${body}${updated}
       </div></div>`
    : `<div class="card-main">
         <div class="card-title">${logoHtml(view)}${name}${badge}</div>
         ${body}
       </div>`;

  return `<div class="card ${cls} ${mode === 'dash' ? 'dash-card' : ''}" data-template="${esc(view.plan.template)}">${inner}</div>`;
}

// ─── 弹窗视图拆分：固定展示 ≤max 家，其余收进"更多" ─────────────
function splitPopupViews(views, popupPlanIds, max) {
  const cap = Math.min(Math.max(max || 2, 1), 10);
  const enabled = views.filter((v) => v.plan.enabled);
  const byId = new Map(enabled.map((v) => [v.plan.id, v]));
  const picked = (popupPlanIds || [])
    .map((id) => byId.get(id))
    .filter(Boolean)
    .slice(0, cap);
  for (const v of enabled) {
    if (picked.length >= cap) break;
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
