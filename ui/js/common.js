// ============================================================
// Plan Limit — 共享渲染层
// 设计规范：字号 11/13/15，字重 400/500/600，过渡 180ms
// 配色：--brand-hue 派生体系（applyAccent 把用户选色转成 Hue）
// ============================================================

const Tauri = window.__TAURI__;
const invoke = Tauri.core.invoke;
const listen = Tauri.event.listen;

// ─── 模板元数据 ────────────────────────────────────────────────
const PROVIDER_META = {
  minimax:        { name: 'MiniMax',       color: '#ff5b4a', icon: 'icons/minimax-color.svg', homepage: 'https://platform.minimaxi.com/console/usage' },
  zhipu:          { name: '智谱 GLM',      color: '#3f7cff', icon: 'icons/m-zai.svg', homepage: 'https://bigmodel.cn/coding-plan/personal/usage' },
  'kimi-coding':  { name: 'Kimi Coding',   color: '#16c8b7', icon: 'icons/kimi-color.png', homepage: 'https://www.kimi.com/code/console' },
  volcengine:     { name: '火山方舟',      color: '#006EFF', icon: 'icons/volcengine-color.png', homepage: 'https://console.volcengine.com/ark/region:ark+cn-beijing/openManagement?advancedActiveKey=subscribe' },
  'claude-official': { name: 'Claude',     color: '#d97757', icon: 'icons/claude-color.svg', homepage: 'https://claude.ai' },
  codex:          { name: 'OpenAI',        color: '#10a37f', icon: 'icons/m-openai.svg', homepage: 'https://chatgpt.com' },
  grok:           { name: 'Grok',          color: '#000000', icon: 'icons/m-grok.svg', homepage: 'https://x.ai/grok' },
  deepseek:       { name: 'DeepSeek',      color: '#4d6bfe', icon: 'icons/deepseek-color.svg', homepage: 'https://platform.deepseek.com' },
  kimi:           { name: 'Kimi',          color: '#0ea5a3', icon: 'icons/kimi-color.png', homepage: 'https://platform.moonshot.cn' },
  stepfun:        { name: '阶跃星辰',      color: '#8b5cf6', icon: 'icons/stepfun-color.svg', homepage: 'https://platform.stepfun.com/plan-subscribe' },
  siliconflow:    { name: '硅基流动',      color: '#6366f1', icon: 'icons/siliconcloud-color.svg', homepage: 'https://cloud.siliconflow.cn' },
  openrouter:     { name: 'OpenRouter',    color: '#C8FF00', icon: 'icons/openrouter-color.svg', homepage: 'https://openrouter.ai/credits' },
  novita:         { name: 'Novita AI',     color: '#23D57C', icon: 'icons/novita-color.svg', homepage: 'https://novita.ai' },
  newapi:         { name: 'NewAPI',        color: '#38bdf8', icon: 'icons/newapi.png', homepage: '' },
  sub2api:        { name: 'Sub2API',       color: '#94a3b8', icon: 'icons/sub2api.svg', homepage: '' },
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

/** 用户选色 → 提取 H/S/L 写入派生变量，全站配色自动跟随；animate 时经 Motion 平滑过渡 */
function hexToHslFull(hex) {
  const n = parseInt(hex.slice(1), 16);
  const r = ((n >> 16) & 255) / 255, g = ((n >> 8) & 255) / 255, b = (n & 255) / 255;
  const max = Math.max(r, g, b), min = Math.min(r, g, b);
  const l = (max + min) / 2;
  const d = max - min;
  if (d === 0) return { h: 257, s: 0, l: Math.round(l * 100) }; // 无色相（黑/白/灰）
  const s = d / (1 - Math.abs(2 * l - 1));
  let h;
  if (max === r) h = ((g - b) / d + (g < b ? 6 : 0)) / 6;
  else if (max === g) h = ((b - r) / d + 2) / 6;
  else h = ((r - g) / d + 4) / 6;
  return { h: Math.round(h * 360), s: Math.round(s * 100), l: Math.round(l * 100) };
}

function applyAccent(hex, animate) {
  const ok = hex && /^#[0-9a-fA-F]{6}$/.test(hex);
  const { h, s, l } = ok ? hexToHslFull(hex) : { h: 252, s: 65, l: 55 };
  const root = document.documentElement;
  root.style.setProperty('--brand-sat', s + '%');
  root.style.setProperty('--brand-light', l + '%');
  if (animate && window.Motion) Motion.hueTo(h);
  else root.style.setProperty('--brand-hue', String(h));
}

function applySettingsLook(s) {
  if (!s) return;
  window.__themePref = s.theme || 'system';
  applyTheme(window.__themePref);
  if (s.accent) applyAccent(s.accent);
  else {
    document.documentElement.style.setProperty('--brand-hue', '262');
    document.documentElement.style.setProperty('--brand-sat', '65%');
    document.documentElement.style.setProperty('--brand-light', '55%');
  }
  window.__barStyle = s.bar_style === 'ring' ? 'ring' : 'bar';
}

async function initTheme() {
  try {
    applySettingsLook(await invoke('get_settings'));
  } catch {
    window.__themePref = 'system';
  }
  applyTheme(window.__themePref);
}

// ─── 工具 ─────────────────────────────────────────────────────
function esc(s) {
  return String(s ?? '').replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));
}

/** 相对时间："刚刚 / 5 分钟前 / 2 小时前" */
function fmtAgo(unixSecs) {
  if (!unixSecs) return '—';
  const diff = Math.floor(Date.now() / 1000) - unixSecs;
  if (diff < 60) return '刚刚';
  if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
  return `${Math.floor(diff / 86400)} 天前`;
}

/** 重置时刻："09-24 01:02" */
function resetAtText(unixSecs) {
  if (!unixSecs) return '';
  const d = new Date(unixSecs * 1000);
  const pad = (n) => String(n).padStart(2, '0');
  return `${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** 重置倒计时："03h02m"，跨天 "1d3h24m"；已过期返回空 */
function countdownText(unixSecs) {
  const secs = Math.floor(unixSecs - Date.now() / 1000);
  if (secs <= 0) return '';
  const m = Math.floor(secs / 60);
  const d = Math.floor(m / 1440);
  const h = Math.floor((m % 1440) / 60);
  const pad = (n) => String(n).padStart(2, '0');
  return d > 0 ? `${d}d${h}h${pad(m % 60)}m` : `${pad(h)}h${pad(m % 60)}m`;
}

function fmtClock(unixSecs) {
  if (!unixSecs) return '—';
  return new Date(unixSecs * 1000).toLocaleTimeString('zh-CN', { hour12: false });
}

function fmtNum(n) {
  if (n >= 1e8) return (n / 1e8).toFixed(1) + '亿';
  if (n >= 1e4) return (n / 1e4).toFixed(1) + '万';
  return String(Math.round(n));
}

/** 规范：Critical = 剩余 <10%（即已用 ≥90%）→ 暖琥珀 */
function isUrgent(usedPct) {
  return usedPct >= 90;
}

// ─── Logo ─────────────────────────────────────────────────────
function metaOf(view) {
  return PROVIDER_META[view.plan.template] || { name: view.plan.template, color: '#8b93a7', icon: '', homepage: '' };
}

function logoHtml(view) {
  const m = metaOf(view);
  const src = view.plan.logo || m.icon;
  if (src) {
    return `<img class="card-logo" src="${esc(src)}" alt="" onerror="this.classList.add('logo-broken')" />`;
  }
  return `<i class="tpl-glyph" style="background:${m.color}"></i>`;
}

// ─── 行渲染（弹窗与主窗口共用） ───────────────────────────────
/**
 * 48px 行：图标 + 名称 + 使用率 + 3px 进度条；hover 淡出详情行。
 * @param opts.sub     "更多"缩进行
 * @param opts.actions 主窗口行右侧控制区（开关/编辑/删除）
 * @param opts.main    主窗口模式：附加深色窗口明细行
 */
/** 规范：窗口排序 5小时 → 周 → 月 → MCP/其他 */
const WIN_ORDER = { '5小时': 0, '5 Hour': 0, '7天': 1, '本周': 1, '周': 1, 'Weekly': 1, '本月': 2, '月': 2, '30天': 2, 'Monthly': 2, '总额度': 0 };
function winPriority(w) {
  const key = Object.keys(WIN_ORDER).find((k) => w.label.toLowerCase() === k.toLowerCase());
  return key !== undefined ? WIN_ORDER[key] : 9;
}

function rowHtml(view, opts = {}) {
  const snap = view.snapshot;
  let right = '';
  let rightCls = '';
  let pctData = '';
  let barPct = -1;
  let tip = '';
  let urgent = false;
  let lines = '';

  if (!snap || !snap.ok) {
    right = '—';
    tip = snap && snap.error ? String(snap.error) : '尚未获取数据';
    if (tip.startsWith('暂不支持')) tip = '暂无窗口额度数据';
  } else {
    const q = snap.quota;
    if (q.kind === 'windows') {
      const wins = [...q.windows].sort((a, b) => winPriority(a) - winPriority(b));
      const worst = wins.reduce((a, w) => Math.max(a, w.used_percent), 0);
      // 窗口明细已含百分比，行头不再重复显示
      urgent = isUrgent(worst)
        || wins.some((w) => 100 - w.used_percent <= (view.plan.threshold ?? 0));
      // 每个窗口一行：标签 + 进度条 + 百分比 + 重置时刻（09-24 01:02）
      lines = wins.map((w) => {
        const u = w.used_percent;
        const reset = w.reset_at ? resetAtText(w.reset_at) : '';
        const urgentLine = isUrgent(u) || (100 - u <= (view.plan.threshold ?? 0)) ? 'urgent' : '';
        // 用尽且有明确重置时刻 → 遮罩倒计时（金额行/无重置时刻不参与）
        const cd = w.reset_at && u >= 100 ? countdownText(w.reset_at) : '';
        return `
        <div class="win-line ${urgentLine}${cd ? ' resetting' : ''}"${cd ? ` data-reset-at="${w.reset_at}" data-countdown="${cd}"` : ''}>
          <span>${esc(w.label)}</span>
          <div class="bar slim ${urgentLine ? 'urgent-fill' : ''}"><i data-w="${u.toFixed(1)}"></i></div>
          <b data-count="${u}" data-fmt="${u < 10 ? 'pct1' : 'pct0'}">${u.toFixed(u < 10 ? 1 : 0)}%</b>
          ${reset ? `<em>${esc(reset)}</em>` : ''}
        </div>`;
      }).join('');
    } else if (q.kind === 'balance') {
      right = `${q.currency === 'CNY' ? '¥' : '$'}${q.amount.toFixed(2)}`;
      pctData = ` data-count="${q.amount}" data-fmt="${q.currency === 'CNY' ? 'cny' : 'usd'}"`;
      // 余额明细走 win-line 结构与窗口行对齐（无 note 时不渲染多余行）
      if (q.note) {
        lines = `
        <div class="win-line">
          <span>明细</span>
          <em style="flex:1;text-align:left">${esc(q.note)}</em>
        </div>`;
      }
      if (q.amount <= view.plan.threshold) { urgent = true; rightCls = 'urgent'; }
    } else if (q.kind === 'fixed_quota') {
      urgent = isUrgent(q.used_percent);
      const u = q.used_percent;
      lines = `
        <div class="win-line ${isUrgent(u) ? 'urgent' : ''}">
          <span>总额度</span>
          <div class="bar slim ${isUrgent(u) ? 'urgent-fill' : ''}"><i data-w="${u.toFixed(1)}"></i></div>
          <b>${u.toFixed(0)}%</b>
        </div>`;
    }
  }
  if (urgent) rightCls = 'urgent';

  const tipLine = tip ? `<div class="row-tip">${esc(tip)}</div>` : '';
  const lead = opts.lead || '';
  const actions = opts.actions || '';
  const logo = logoHtml(view);

  return `
  <div class="row ${opts.sub ? 'row-sub' : ''} ${urgent ? 'row-urgent' : ''}" data-template="${esc(view.plan.template)}">
    <div class="row-main">
      ${lead ? `<div class="row-lead">${lead}</div>` : ''}
      <div class="row-icon">${logo}</div>
      <div class="row-body">
        <div class="row-top">
          <span class="row-name">${esc(view.plan.name)}${view.plan.enabled ? '' : ' <em class="row-off">已停用</em>'}</span>
          ${right ? `<span class="row-pct ${rightCls}"${pctData}>${esc(right)}</span>` : ''}
        </div>
        ${lines}
      </div>
      ${actions ? `<div class="row-tail">${actions}</div>` : ''}
    </div>
    ${tipLine}
  </div>`;
}


// ─── 弹窗视图拆分：未选择时默认固定 3 家；选了 N 家展示 N 家（≤10） ──
function splitPopupViews(views, popupPlanIds, max) {
  const ids = popupPlanIds || [];
  const cap = ids.length ? Math.min(Math.max(ids.length, 1), 10) : Math.min(Math.max(max || 3, 1), 3);
  const enabled = views.filter((v) => v.plan.enabled);
  // 展示顺序跟随套餐列表顺序（与主窗口拖拽排序一致），而非勾选顺序
  const picked = enabled
    .filter((v) => ids.includes(v.plan.id))
    .slice(0, cap);
  for (const v of enabled) {
    if (picked.length >= cap) break;
    if (!picked.includes(v)) picked.push(v);
  }
  const rest = enabled.filter((v) => !picked.includes(v));
  return { primary: picked, rest };
}

/** 渲染后驱动：行入场瀑布 + 进度条填充 + 数字滚动（每次刷新都有"数据到达"的生命感） */
function animateBars(container) {
  if (window.Motion) {
    Motion.rowsIn(container);
    Motion.dataIn(container);
    return;
  }
  requestAnimationFrame(() => requestAnimationFrame(() => {
    container.querySelectorAll('.bar > i[data-w]').forEach((el) => {
      el.style.width = el.dataset.w + '%';
      el.removeAttribute('data-w');
    });
  }));
}

// ─── Toast（仅错误提示；规范禁止成功提示） ────────────────────
let _toastTimer = null;
function toast(msg, warn) {
  let el = document.getElementById('toast');
  if (!el) {
    el = document.createElement('div');
    el.id = 'toast';
    document.body.appendChild(el);
  }
  el.textContent = msg;
  el.classList.add('show');
  clearTimeout(_toastTimer);
  _toastTimer = setTimeout(() => el.classList.remove('show'), 3200);
}

// ─── 遮罩倒计时滚动 ───────────────────────────────────────────
// 只改 data-countdown 文本，不重渲染；归零解除遮罩（等后端轮询出新数据）
setInterval(() => {
  document.querySelectorAll('.win-line.resetting').forEach((el) => {
    const t = countdownText(parseFloat(el.dataset.resetAt));
    if (t) el.dataset.countdown = t;
    else el.classList.remove('resetting');
  });
}, 30000);
