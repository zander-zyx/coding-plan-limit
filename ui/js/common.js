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
  minimax:        { name: 'MiniMax',       color: '#ff5b4a', icon: 'icons/minimax.png', homepage: 'https://platform.minimaxi.com' },
  zhipu:          { name: '智谱 GLM',      color: '#3f7cff', icon: 'icons/zai.svg', homepage: 'https://open.bigmodel.cn' },
  'kimi-coding':  { name: 'Kimi Coding',   color: '#16c8b7', icon: 'icons/kimi.png', homepage: 'https://www.kimi.com/coding' },
  'claude-official': { name: 'Claude',     color: '#d97757', icon: 'icons/claude-official.png', homepage: 'https://claude.ai' },
  codex:          { name: 'Codex',         color: '#10a37f', icon: 'icons/codex.png', homepage: 'https://chatgpt.com' },
  'claude-cache': { name: 'Claude',        color: '#d97757', icon: 'icons/claude-cache.svg', homepage: 'https://claude.ai' },
  xiaomi:         { name: '小米 MiMo',     color: '#ff6900', icon: 'icons/xiaomi.ico', homepage: 'https://platform.xiaomimimo.com' },
  deepseek:       { name: 'DeepSeek',      color: '#4d6bfe', icon: 'icons/deepseek.ico', homepage: 'https://platform.deepseek.com' },
  kimi:           { name: 'Kimi',          color: '#0ea5a3', icon: 'icons/kimi.png', homepage: 'https://platform.moonshot.cn' },
  stepfun:        { name: '阶跃星辰',      color: '#8b5cf6', icon: 'icons/stepfun.png', homepage: 'https://platform.stepfun.com' },
  siliconflow:    { name: '硅基流动',      color: '#6366f1', icon: 'icons/siliconflow.png', homepage: 'https://cloud.siliconflow.cn' },
  alibaba:        { name: '阿里云',        color: '#f59e0b', icon: 'icons/alibaba.png', homepage: 'https://bailian.console.aliyun.com' },
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

/** Kimi 规范：Hue 落在 260–280° 时吸附到官方 Kimi 紫 #7c5cff（hue≈257） */
function hexToHue(hex) {
  const n = parseInt(hex.slice(1), 16);
  const r = ((n >> 16) & 0xff) / 255, g = ((n >> 8) & 0xff) / 255, b = (n & 0xff) / 255;
  const max = Math.max(r, g, b), min = Math.min(r, g, b);
  if (max === min) return 257;
  const d = max - min;
  let h;
  if (max === r) h = ((g - b) / d + (g < b ? 6 : 0)) / 6;
  else if (max === g) h = ((b - r) / d + 2) / 6;
  else h = ((r - g) / d + 4) / 6;
  const deg = Math.round(h * 360);
  return deg >= 260 && deg <= 280 ? 257 : deg;
}

/** 用户选色 → 提取 Hue 写入 --brand-hue，全站配色自动派生 */
function applyAccent(hex) {
  if (!hex || !/^#[0-9a-fA-F]{6}$/.test(hex)) return;
  document.documentElement.style.setProperty('--brand-hue', String(hexToHue(hex)));
}

function applySettingsLook(s) {
  if (!s) return;
  window.__themePref = s.theme || 'system';
  applyTheme(window.__themePref);
  if (s.accent) applyAccent(s.accent);
  else document.documentElement.style.setProperty('--brand-hue', '262');
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
  let right = '—';
  let rightCls = '';
  let barPct = -1;
  let tip = '';
  let urgent = false;
  let lines = '';

  if (!snap || !snap.ok) {
    tip = snap && snap.error ? String(snap.error) : '尚未获取数据';
    if (tip.startsWith('暂不支持')) tip = '暂无窗口额度数据';
  } else {
    const q = snap.quota;
    if (q.kind === 'windows') {
      const wins = [...q.windows].sort((a, b) => winPriority(a) - winPriority(b));
      const worst = wins.reduce((a, w) => Math.max(a, w.used_percent), 0);
      // 主窗口模式：汇总大数字多余（窗口行里已有），只在弹窗显示
      if (!opts.hideHeadPct) right = `${worst.toFixed(worst < 10 ? 1 : 0)}%`;
      urgent = isUrgent(worst);
      // 每个窗口一行：标签 + 进度条 + 百分比 + 重置时刻（09-24 01:02）
      lines = wins.map((w) => {
        const u = w.used_percent;
        const reset = w.reset_at ? resetAtText(w.reset_at) : '';
        const urgentLine = isUrgent(u) ? 'urgent' : '';
        return `
        <div class="win-line ${urgentLine}">
          <span>${esc(w.label)}</span>
          <div class="bar slim ${urgentLine}"><i data-w="${u.toFixed(1)}"></i></div>
          <b>${u.toFixed(u < 10 ? 1 : 0)}%</b>
          ${reset ? `<em>${esc(reset)}</em>` : ''}
        </div>`;
      }).join('');
    } else if (q.kind === 'balance') {
      if (!opts.hideHeadPct) right = `${q.currency === 'CNY' ? '¥' : '$'}${q.amount.toFixed(2)}`;
      // 余额明细也走 win-line 结构，与窗口行左对齐（标签列留空）
      if (q.note) {
        lines = `
        <div class="win-line">
          <span>明细</span>
          <em style="flex:1;text-align:left">${esc(q.note)}</em>
        </div>`;
      } else {
        tip = q.currency;
      }
      if (q.amount <= view.plan.threshold) { urgent = true; rightCls = 'urgent'; }
    } else if (q.kind === 'fixed_quota') {
      if (!opts.hideHeadPct) right = `${q.used_percent.toFixed(0)}%`;
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
  const actions = opts.actions || '';
  const logo = logoHtml(view);

  return `
  <div class="row ${opts.sub ? 'row-sub' : ''} ${urgent ? 'row-urgent' : ''}" data-template="${esc(view.plan.template)}">
    ${actions ? `<div class="row-lead">${actions}</div>` : ''}
    <div class="row-main">
      <div class="row-icon">${logo}</div>
      <div class="row-body">
        <div class="row-top">
          <span class="row-name">${esc(view.plan.name)}${view.plan.enabled ? '' : ' <em class="row-off">已停用</em>'}</span>
          <span class="row-pct ${rightCls}">${esc(right)}</span>
        </div>
        ${lines}
      </div>
    </div>
    ${tipLine}
  </div>`;
}

/** 主窗口行右侧控制区（套餐管理） */
function rowActionsHtml(view) {
  return `
  <div class="row-actions">
    <label class="switch" title="启用/停用">
      <input type="checkbox" data-toggle="${view.plan.id}" ${view.plan.enabled ? 'checked' : ''} /><i></i>
    </label>
    <button class="txt-btn" data-edit="${view.plan.id}">编辑</button>
    <button class="txt-btn danger" data-del="${view.plan.id}">删除</button>
  </div>`;
}

// ─── 弹窗视图拆分：未选择时默认固定 3 家；选了 N 家展示 N 家（≤10） ──
function splitPopupViews(views, popupPlanIds, max) {
  const ids = popupPlanIds || [];
  const cap = ids.length ? Math.min(Math.max(ids.length, 1), 10) : Math.min(Math.max(max || 3, 1), 3);
  const enabled = views.filter((v) => v.plan.enabled);
  const byId = new Map(enabled.map((v) => [v.plan.id, v]));
  const picked = ids
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

/** 渲染后触发进度条从 0 → 目标值的填充动画（每次刷新都有生命感） */
function animateBars(container) {
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
