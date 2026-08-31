// 主窗口逻辑：套餐 / 设置 / 关于（更改即时保存，无成功提示）
const $ = (id) => document.getElementById(id);

const state = {
  views: [],
  templates: [],
  settings: null,
  editingId: null,
  selectedTpl: null,
  tplVariant: 'kimi',
  pendingLogo: null,
  nameDirty: false, // 套餐名称是否被用户手动改过：没改过则随模板切换刷新默认名
  accentPresets: ['#7c5cff', '#3f7cff', '#16c8b7', '#f59e0b', '#fb7185', '#0ea5a3', '#000000'],
};

// ─── 视图切换 ─────────────────────────────────────────────────
document.querySelectorAll('.side nav button').forEach((btn) => {
  btn.addEventListener('click', () => {
    document.querySelectorAll('.side nav button').forEach((b) => b.classList.remove('active'));
    btn.classList.add('active');
    ['plans', 'settings', 'about'].forEach((v) => {
      $(`view-${v}`).hidden = v !== btn.dataset.view;
    });
    if (window.Motion) Motion.viewIn($(`view-${btn.dataset.view}`));
  });
});

// ─── 套餐列表（行式，弹窗同构） ───────────────────────────────
function renderPlans() {
  try {
    const enabled = state.views.filter((v) => v.plan.enabled);
    const bad = enabled.filter((v) => v.snapshot && !v.snapshot.ok).length;
    $('plans-sub').textContent = state.views.length
      ? `共 ${state.views.length} · 启用 ${enabled.length}${bad ? ` · 异常 ${bad}` : ''}`
      : '尚未添加套餐';

    const box = $('plan-list');
    if (!state.views.length) {
      box.innerHTML = `<div class="p-empty" style="padding:80px 0">点击右上角「添加套餐」开始</div>`;
      return;
    }
    box.innerHTML = state.views.map((v) => {
      const lead = `
        <div class="row-lead">
          <label class="switch sm" title="启用/停用">
            <input type="checkbox" data-toggle="${v.plan.id}" ${v.plan.enabled ? 'checked' : ''} /><i></i>
          </label>
        </div>`;
      const actions = `
        <div class="row-actions">
          <button class="txt-btn" data-edit="${v.plan.id}">编辑</button>
          <button class="txt-btn" data-del="${v.plan.id}">删除</button>
        </div>`;
      return rowHtml(v, { lead, actions });
    }).join('');
    animateBars(box);

    box.querySelectorAll('.row-tip').forEach((el) => { el.style.display = 'block'; });
    setupDrag(box);

    box.querySelectorAll('[data-toggle]').forEach((el) =>
      el.addEventListener('change', () => togglePlan(el.dataset.toggle, el.checked)));
    box.querySelectorAll('[data-edit]').forEach((el) =>
      el.addEventListener('click', () => openModal(el.dataset.edit)));
    box.querySelectorAll('[data-del]').forEach((el) =>
      el.addEventListener('click', () => {
        const view = state.views.find((v) => v.plan.id === el.dataset.del);
        askDeletePlan(el.dataset.del, view?.plan.name || '');
      }));
  } catch (e) {
    $('plan-list').innerHTML = `<div class="p-empty" style="color:var(--urgent)">列表渲染异常：${esc(String(e && e.message || e))}</div>`;
    console.error('renderPlans failed:', e);
  }
}

async function togglePlan(id, enabled) {
  const view = state.views.find((v) => v.plan.id === id);
  if (!view) return;
  view.plan.enabled = enabled;
  try {
    await invoke('save_plan', { plan: { ...view.plan }, secret: null });
    // 静默切换：不 reload 不闪动；弹窗打开时自行拉最新
  } catch (e) {
    view.plan.enabled = !enabled;
    toast(String(e));
    await reload(); // 失败回滚 UI
  }
}

async function removePlan(id) {
  try {
    await invoke('delete_plan', { id });
  } catch (e) {
    toast(String(e));
  }
  await reload();
}

// ─── 拖拽排序（原生 HTML5 drag；顺序持久化并决定弹窗补足顺序） ──
function setupDrag(box) {
  let dragId = null;
  let dragFromIdx = -1;
  box.querySelectorAll('.row').forEach((row, idx) => {
    row.dataset.idx = String(idx);
    row.setAttribute('draggable', 'true');
    row.addEventListener('dragstart', (e) => {
      dragId = row.querySelector('[data-toggle]')?.dataset.toggle
        || row.querySelector('[data-edit]')?.dataset.edit;
      dragFromIdx = idx;
      row.classList.add('dragging');
      e.dataTransfer.effectAllowed = 'move';
      e.dataTransfer.setData('text/plain', dragId || '');
    });
    row.addEventListener('dragend', () => {
      row.classList.remove('dragging');
      box.querySelectorAll('.row').forEach((r) => r.classList.remove('drag-over'));
    });
    row.addEventListener('dragover', (e) => {
      e.preventDefault();
      e.dataTransfer.dropEffect = 'move';
      if (!dragId) return;
      const overId = row.querySelector('[data-toggle]')?.dataset.toggle
        || row.querySelector('[data-edit]')?.dataset.edit;
      if (overId && overId !== dragId) row.classList.add('drag-over');
    });
    row.addEventListener('dragleave', () => row.classList.remove('drag-over'));
    row.addEventListener('drop', async (e) => {
      e.preventDefault();
      row.classList.remove('drag-over');
      const dragRow = box.querySelector('.row.dragging');
      const overId = row.querySelector('[data-toggle]')?.dataset.toggle
        || row.querySelector('[data-edit]')?.dataset.edit;
      if (!dragId || !overId || dragId === overId || !dragRow) { dragId = null; return; }
      // 向下拖 → 放到目标之后；向上拖 → 放到目标之前
      if (dragFromIdx >= 0 && idx > dragFromIdx) {
        row.after(dragRow);
      } else {
        box.insertBefore(dragRow, row);
      }
      const ids = [...box.querySelectorAll('.row')]
        .map((r) => r.querySelector('[data-toggle]')?.dataset.toggle
          || r.querySelector('[data-edit]')?.dataset.edit)
        .filter(Boolean);
      dragId = null;
      dragFromIdx = -1;
      try {
        await invoke('reorder_plans', { ids });
      } catch (err) {
        toast(String(err));
      }
      await reload();
    });
  });
}

// ─── 添加/编辑弹层 ────────────────────────────────────────────
// 品牌短名（模板卡与默认套餐名）
const SHORT_LABEL = {
  minimax: 'MiniMax', zhipu: '智谱GLM', 'kimi-coding': 'Kimi', 'claude-official': 'Claude',
  codex: 'OpenAI', deepseek: 'DeepSeek',
  kimi: 'Kimi', stepfun: '阶跃', siliconflow: '硅基流动',
  newapi: 'NewAPI', sub2api: 'Sub2API',
};
const shortLabel = (id) => SHORT_LABEL[id] || (PROVIDER_META[id] || {}).name || id;

function renderTplGrid() {
  $('tpl-grid').innerHTML = state.templates.filter((t) => t.id !== 'kimi-coding').map((t) => {
    const m = PROVIDER_META[t.id] || { color: '#8b93a7', icon: '' };
    const label = shortLabel(t.id);
    const glyph = m.icon
      ? `<img class="t-logo" src="${esc(m.icon)}" alt="" />`
      : `<i style="background:${m.color};width:7px;height:7px;border-radius:2px;transform:rotate(45deg);flex:none"></i>`;
    return `
    <button class="tpl-item ${state.selectedTpl === t.id ? 'active' : ''}" data-tpl="${t.id}">
      <div class="t-name">${glyph}${esc(label)}</div>
    </button>`;
  }).join('');
  $('tpl-grid').querySelectorAll('[data-tpl]').forEach((el) =>
    el.addEventListener('click', () => {
      state.selectedTpl = el.dataset.tpl;
      if (state.selectedTpl === 'kimi' && !state.tplVariant) state.tplVariant = 'kimi';
      renderTplGrid();
      renderFormFields();
      $('plan-form').hidden = false;
      validateForm();
    }));
}

function currentTpl() {
  const id = state.selectedTpl === 'kimi' ? state.tplVariant : state.selectedTpl;
  return state.templates.find((t) => t.id === id);
}

function renderFormFields() {
  const t = currentTpl();
  if (!t) return;
  const editing = state.editingId && state.views.find((v) => v.plan.id === state.editingId);
  state.pendingLogo = editing ? (editing.plan.logo || null) : null;
  // 切换模板/变体时保留用户已输入但未保存的内容
  const keepName = $('f-name').value;
  const keepThreshold = $('f-threshold').value;
  const keepBaseUrl = $('f-baseurl').value;

  $('f-region-item').hidden = !t.has_region;
  $('f-kimi-variant-item').hidden = state.selectedTpl !== 'kimi';
  $('seg-kimi-variant').querySelectorAll('button').forEach((b) =>
    b.classList.toggle('active', b.dataset.v === state.tplVariant));
  $('f-bearer-item').hidden = t.auth !== 'bearer';
  $('f-cookie-item').hidden = t.auth !== 'cookie';
  $('f-cookie-key-item').hidden = t.auth !== 'cookie';
  $('f-ak-item').hidden = t.auth !== 'bss';
  $('f-ak-secret-item').hidden = t.auth !== 'bss';
  $('ak-hint').textContent = t.id === 'volcengine'
    ? '火山引擎账号 AccessKey（IAM 密钥，非方舟推理 API Key），获取：控制台右上角头像 → 访问控制 → API 访问密钥'
    : '阿里云主账号 RAM 密钥（非 DashScope API Key）';
  $('f-baseurl-item').hidden = !t.needs_base_url;
  $('f-baseurl-hint').textContent = t.id === 'zhipu'
    ? '默认 https://open.bigmodel.cn/api，国际站 https://api.z.ai/api'
    : '必填，例如 https://your-newapi-site.com';
  // Codex 多账号区块（仅 codex 模板；新建套餐须先保存拿到 plan_id 才能绑定）
  const isCodex = t.id === 'codex';
  $('f-codex-item').hidden = !isCodex;
  if (isCodex) {
    resetCodexLoginBox();
    const locked = !state.editingId;
    // 登录新账号/绑定下拉是全局操作（入托管账号库），不依赖套餐保存；捕获/恢复跟随需要 plan_id
    ['btn-codex-capture', 'btn-codex-follow']
      .forEach((id) => ($(id).disabled = locked));
    $('f-codex-hint').textContent = locked
      ? '保存套餐后即可绑定账号；可先登录新账号。'
      : '默认跟随本机 Codex CLI 当前登录的账号。';
    refreshCodexAccounts();
  }

  // 百分比型：滑杆 + % 尾缀自解释，不放说明文字；余额型：货币无量程，保留数字框 + 短 hint
  const pctType = t.quota_type !== 'balance';
  $('f-threshold-label').textContent = pctType ? '提醒阈值（剩余）' : '余额提醒下限';
  $('f-threshold-range').hidden = !pctType;
  $('f-threshold-unit').hidden = !pctType;
  $('f-threshold-hint').hidden = pctType;
  $('f-threshold-hint').textContent = pctType ? '' : '账户余额低于该数值时触发系统通知';

  if (editing) {
    $('f-name').value = keepName || editing.plan.name;
    $('f-region').value = editing.plan.region || 'cn';
    $('f-baseurl').value = keepBaseUrl || editing.plan.base_url || '';
    $('f-threshold').value = keepThreshold || editing.plan.threshold;
    $('f-enabled').checked = editing.plan.enabled;
    const ph = '已配置，留空保持不变';
    $('f-bearer').placeholder = ph;
    $('f-cookie').placeholder = ph;
    $('f-cookie-key').placeholder = ph;
    $('f-ak-id').placeholder = ph;
    $('f-ak-secret').placeholder = ph;
  } else {
    $('f-name').value = state.nameDirty ? keepName : shortLabel(t.id);
    $('f-region').value = 'cn';
    $('f-baseurl').value = keepBaseUrl;
    $('f-threshold').value = keepThreshold || 10;
    $('f-enabled').checked = true;
    ['f-bearer', 'f-cookie', 'f-cookie-key', 'f-ak-id', 'f-ak-secret'].forEach((id) => ($(id).placeholder = ''));
  }
  syncThreshold();
  renderLogoPreview();
}

// 数字框 → 滑杆同步（仅百分比型；只改滑杆显示，不打断用户输入）
function syncThreshold() {
  const range = $('f-threshold-range');
  if (range.hidden) return;
  const raw = parseFloat($('f-threshold').value);
  range.value = Number.isFinite(raw) ? Math.min(100, Math.max(0, raw)) : 0;
}

function renderLogoPreview() {
  const t = currentTpl();
  const preview = $('f-logo-preview');
  if (state.pendingLogo) {
    preview.src = state.pendingLogo;
    preview.hidden = false;
    $('btn-logo-reset').hidden = false;
  } else {
    const m = t ? (PROVIDER_META[t.id] || {}) : {};
    if (m.icon) {
      preview.src = m.icon;
      preview.hidden = false;
    } else {
      preview.hidden = true;
    }
    $('btn-logo-reset').hidden = true;
  }
}

$('btn-logo-pick').addEventListener('click', () => {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = 'image/png,image/jpeg,image/webp,image/*';
  input.onchange = () => {
    const file = input.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      const img = new Image();
      img.onload = () => {
        const canvas = document.createElement('canvas');
        canvas.width = 96;
        canvas.height = 96;
        const ctx = canvas.getContext('2d');
        const side = Math.min(img.width, img.height);
        ctx.drawImage(img, (img.width - side) / 2, (img.height - side) / 2, side, side, 0, 0, 96, 96);
        state.pendingLogo = canvas.toDataURL('image/png');
        renderLogoPreview();
      };
      img.src = reader.result;
    };
    reader.readAsDataURL(file);
  };
  input.click();
});
$('btn-logo-reset').addEventListener('click', () => {
  state.pendingLogo = null;
  renderLogoPreview();
});

function validateForm() {
  $('btn-save').disabled = !state.selectedTpl || !$('f-name').value.trim();
}

function openModal(planId) {
  state.editingId = planId || null;
  state.nameDirty = false; // 每次打开都是新交互，名称重新随模板/原套餐名
  const editing = planId ? state.views.find((v) => v.plan.id === planId) : null;
  const tplId = planId ? editing?.plan.template : null;
  if (tplId === 'kimi' || tplId === 'kimi-coding') {
    state.selectedTpl = 'kimi';
    state.tplVariant = tplId;
  } else {
    state.selectedTpl = tplId;
    state.tplVariant = 'kimi';
  }
  $('modal-title').textContent = planId ? '编辑套餐' : '添加套餐';
  $('btn-delete').hidden = !planId;
  $('plan-form').hidden = !planId;
  renderTplGrid();
  if (planId) renderFormFields();
  validateForm();
  $('plan-modal').classList.add('show');
  if (window.Motion) Motion.modalIn($('plan-modal').querySelector('.modal'));
}

function closeModal() {
  // 退场动画播完再清表单（渐隐中内容保持可读）；未接管时同步清理
  const cleanup = () => {
    ['f-name', 'f-bearer', 'f-cookie', 'f-cookie-key', 'f-ak-id', 'f-ak-secret', 'f-baseurl']
      .forEach((id) => ($(id).value = ''));
    state.editingId = null;
    state.selectedTpl = null;
    state.tplVariant = 'kimi';
    if (typeof resetCodexLoginBox === 'function') resetCodexLoginBox();
    state.pendingLogo = null;
  };
  $('plan-modal').classList.remove('show');
  if (!(window.Motion && Motion.modalOut($('plan-modal').querySelector('.modal'), cleanup))) cleanup();
}

$('btn-add').addEventListener('click', () => openModal(null));
$('btn-cancel').addEventListener('click', closeModal);
$('plan-modal').addEventListener('click', (e) => {
  if (e.target === $('plan-modal')) closeModal();
});
$('f-name').addEventListener('input', () => {
  state.nameDirty = true;
  validateForm();
});

// 阈值：滑杆 ⇄ 数字框双向联动
$('f-threshold-range').addEventListener('input', () => {
  $('f-threshold').value = $('f-threshold-range').value;
});
$('f-threshold').addEventListener('input', syncThreshold);

$('seg-kimi-variant').addEventListener('click', (e) => {
  const v = e.target.dataset?.v;
  if (v) {
    state.tplVariant = v;
    renderFormFields();
  }
});

$('btn-save').addEventListener('click', async () => {
  const t = currentTpl();
  if (!t) return;
  const editing = state.editingId && state.views.find((v) => v.plan.id === state.editingId);

  const plan = {
    id: state.editingId || '',
    template: t.id,
    name: $('f-name').value.trim(),
    enabled: $('f-enabled').checked,
    threshold: (() => {
      const raw = parseFloat($('f-threshold').value);
      if (!Number.isFinite(raw)) return t.quota_type === 'balance' ? 0 : 10;
      return t.quota_type === 'balance' ? Math.max(0, raw) : Math.min(100, Math.max(0, raw));
    })(),
    region: t.has_region ? $('f-region').value : 'cn',
    base_url: t.needs_base_url ? $('f-baseurl').value.trim() || null : null,
    logo: state.pendingLogo || null,
    created_at: editing ? editing.plan.created_at : Math.floor(Date.now() / 1000),
  };
  const secret = {
    bearer: t.auth === 'bearer' ? $('f-bearer').value : (t.auth === 'cookie' ? $('f-cookie-key').value : null),
    cookie: t.auth === 'cookie' ? $('f-cookie').value : null,
    ak_id: t.auth === 'bss' ? $('f-ak-id').value : null,
    ak_secret: t.auth === 'bss' ? $('f-ak-secret').value : null,
  };

  try {
    const out = await invoke('save_plan', { plan, secret });
    if (out.warning) toast(out.warning);
    closeModal();
  } catch (e) {
    toast(String(e));
  }
  await reload();
});

$('btn-delete').addEventListener('click', () => {
  const view = state.views.find((v) => v.plan.id === state.editingId);
  if (view) askDeletePlan(view.plan.id, view.plan.name);
});

// ─── 删除确认弹层（行内删除与编辑弹窗删除共用） ──────────────────────────
function askDeletePlan(id, name) {
  $('confirm-name').textContent = name || '该套餐';
  $('confirm-modal').dataset.planId = id;
  $('confirm-modal').classList.add('show');
  if (window.Motion) Motion.modalIn($('confirm-modal').querySelector('.modal'));
}
function closeConfirm() {
  $('confirm-modal').classList.remove('show');
}
$('btn-confirm-cancel').addEventListener('click', closeConfirm);
$('confirm-modal').addEventListener('click', (e) => {
  if (e.target === $('confirm-modal')) closeConfirm();
});
$('btn-confirm-ok').addEventListener('click', async () => {
  const id = $('confirm-modal').dataset.planId;
  closeConfirm();
  await removePlan(id);
  if (state.editingId === id) closeModal();
});

// ─── 设置 ─────────────────────────────────────────────────────
function renderSettings() {
  const s = state.settings;
  if (!s) return;

  $('seg-theme').querySelectorAll('button').forEach((b) =>
    b.classList.toggle('active', b.dataset.v === (s.theme || 'system')));

  $('accent-presets').innerHTML = state.accentPresets
    .map((c) => `<button class="accent-swatch ${((s.accent || '').toLowerCase() === c.toLowerCase()) ? 'active' : ''}" data-c="${c}" style="background:${c}" title="${c}"></button>`)
    .join('');
  $('accent-presets').querySelectorAll('[data-c]').forEach((b) =>
    b.addEventListener('click', () => saveSettings({ accent: b.dataset.c }).then(renderSettings)));
  $('accent-picker').value = s.accent || '#7c5cff';
  $('accent-hex').value = (s.accent || '#7c5cff').toUpperCase();
  // 饱和度/对比度滑杆：从当前主题色反推
  const curHsl = hexToHslFull(s.accent || '#7c5cff');
  $('accent-sat').value = curHsl.s;
  $('accent-sat-num').value = curHsl.s;
  $('accent-light').value = curHsl.l;
  $('accent-light-num').value = curHsl.l;

  const mode = s.notify_mode || 'interval';
  $('seg-notify').querySelectorAll('button').forEach((b) =>
    b.classList.toggle('active', b.dataset.v === mode));
  $('row-notify-interval').hidden = mode !== 'interval';
  $('row-notify-count').hidden = mode !== 'count';
  $('in-notify-interval').value = s.notify_interval_minutes ?? 60;
  $('in-notify-count').value = s.notify_count ?? 10;

  $('in-refresh').value = s.refresh_seconds ?? 30;
  $('in-popup-cooldown').value = s.popup_cooldown_secs ?? 8;
  $('in-autostart').checked = !!s.autostart;
  $('in-auto-update').checked = s.auto_check_update !== false;

  const enabledViews = state.views.filter((v) => v.plan.enabled);
  const picked = new Set(s.popup_plan_ids || []);
  $('popup-pick').innerHTML = enabledViews.length
    ? enabledViews.map((v) => {
        const m = metaOf(v);
        return `
        <label class="pick-row">
          <input type="checkbox" data-pick="${v.plan.id}" ${picked.has(v.plan.id) ? 'checked' : ''} />
          ${logoHtml(v)}${esc(v.plan.name)}
        </label>`;
      }).join('') + `<div class="pick-hint" id="pick-hint"></div>`
    : `<div class="pick-hint">暂无启用的套餐</div>`;
  const updatePickHint = () => {
    const n = $('popup-pick').querySelectorAll('[data-pick]:checked').length;
    const hint = $('pick-hint');
    if (hint) hint.textContent = `已选 ${n}/10 · 未选时默认固定 3 家`;
  };
  updatePickHint();
  $('popup-pick').querySelectorAll('[data-pick]').forEach((el) =>
    el.addEventListener('change', () => {
      let ids = [...$('popup-pick').querySelectorAll('[data-pick]:checked')]
        .map((x) => x.dataset.pick);
      if (ids.length > 10) {
        el.checked = false;
        ids = ids.slice(0, 10);
        toast('最多固定展示 10 家');
        updatePickHint();
        return;
      }
      updatePickHint();
      saveSettings({ popup_plan_ids: ids });
    }));

  // 以 logo_style 为准（custom_icon 只是自定义图存档，切内置样式时保留）；
  // 存量 mono 已移除，custom 无图回落原色
  let logoStyle = ['color', 'mark', 'custom'].includes(s.logo_style) ? s.logo_style : 'color';
  if (logoStyle === 'custom' && !s.custom_icon) logoStyle = 'color';
  $('seg-logo-style').querySelectorAll('button').forEach((b) =>
    b.classList.toggle('active', b.dataset.v === logoStyle));
  // 「选择图片」常驻：任何样式下都可换图，选完自动落到自定义
  // 自定义选项本身即预览位：有图显图，无图显 +
  const thumb = $('custom-logo-thumb');
  const plus = $('custom-logo-plus');
  if (s.custom_icon) {
    thumb.src = s.custom_icon;
    thumb.hidden = false;
    plus.hidden = true;
  } else {
    thumb.hidden = true;
    plus.hidden = false;
  }
}

async function saveSettings(patch) {
  const merged = { ...state.settings, ...patch };
  try {
    await invoke('save_settings', { settings: merged });
    state.settings = merged;
    if (patch.theme !== undefined) {
      window.__themePref = patch.theme;
      applyTheme(patch.theme);
    }
    if (patch.accent !== undefined) applyAccent(patch.accent, true);
  } catch (e) {
    toast(String(e));
  }
}

$('seg-theme').addEventListener('click', (e) => {
  const v = e.target.dataset?.v;
  if (v) saveSettings({ theme: v }).then(renderSettings);
});

$('seg-notify').addEventListener('click', (e) => {
  const v = e.target.dataset?.v;
  if (v) saveSettings({ notify_mode: v }).then(renderSettings);
});
$('in-notify-interval').addEventListener('change', (e) =>
  saveSettings({ notify_interval_minutes: Math.max(1, parseInt(e.target.value) || 60) }));
$('in-notify-count').addEventListener('change', (e) =>
  saveSettings({ notify_count: Math.max(1, parseInt(e.target.value) || 10) }));

$('in-refresh').addEventListener('change', (e) =>
  saveSettings({ refresh_seconds: Math.max(10, parseInt(e.target.value) || 30) }));
$('in-popup-cooldown').addEventListener('change', (e) =>
  saveSettings({ popup_cooldown_secs: Math.min(120, Math.max(0, parseInt(e.target.value) || 0)) }));
$('in-autostart').addEventListener('change', (e) =>
  saveSettings({ autostart: e.target.checked }));
$('in-auto-update').addEventListener('change', (e) =>
  saveSettings({ auto_check_update: e.target.checked }));

$('accent-picker').addEventListener('change', (e) => {
  const v = e.target.value;
  if (/^#[0-9a-fA-F]{6}$/.test(v)) {
    $('accent-hex').value = v.toUpperCase();
    saveSettings({ accent: v }).then(renderSettings);
  }
});
$('accent-hex').addEventListener('change', (e) => {
  const v = e.target.value.trim().replace(/^#?([0-9a-fA-F]{6})$/, '#$1');
  if (/^#[0-9a-fA-F]{6}$/.test(v)) {
    saveSettings({ accent: v }).then(renderSettings);
  } else {
    toast('颜色格式应为 #RRGGBB');
    e.target.value = (state.settings.accent || '#7C5CFF').toUpperCase();
  }
});
$('accent-reset').addEventListener('click', () => saveSettings({ accent: null }).then(renderSettings));

// ─── 主题色饱和度/对比度滑杆：实时合成 HSL → hex 预览，change 落盘 ─────────
function hslToHex(h, s, l) {
  s /= 100; l /= 100;
  const k = (n) => (n + h / 30) % 12;
  const a = s * Math.min(l, 1 - l);
  const f = (n) => l - a * Math.max(-1, Math.min(k(n) - 3, Math.min(9 - k(n), 1)));
  const to = (x) => Math.round(f(x) * 255).toString(16).padStart(2, '0');
  return `#${to(0)}${to(8)}${to(4)}`.toUpperCase();
}

function applyAccentFromSliders() {
  const hue = hexToHslFull($('accent-hex').value || '#7c5cff').h;
  const hex = hslToHex(hue, +$('accent-sat').value, +$('accent-light').value);
  $('accent-hex').value = hex;
  applyAccent(hex, false); // 拖动跟手，无动画
  return hex;
}

[['accent-sat', 'accent-sat-num'], ['accent-light', 'accent-light-num']].forEach(([range, num]) => {
  $(range).addEventListener('input', () => {
    $(num).value = $(range).value;
    applyAccentFromSliders();
  });
  $(num).addEventListener('input', () => {
    const v = Math.min(100, Math.max(0, parseInt($(num).value) || 0));
    $(range).value = v;
    applyAccentFromSliders();
  });
  // 拖动/输入结束才持久化（input 期间只做实时预览）
  [range, num].forEach((id) => $(id).addEventListener('change', () => {
    saveSettings({ accent: applyAccentFromSliders() });
  }));
});

// ─── Logo（原色 / Mark / 自定义图片，托盘+标题栏+侧边栏同步） ──
function pickLogoImage() {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = 'image/png,image/jpeg,image/webp,image/*';
  input.onchange = () => {
    const file = input.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      const img = new Image();
      img.onload = async () => {
        const canvas = document.createElement('canvas');
        canvas.width = 128;
        canvas.height = 128;
        const ctx = canvas.getContext('2d');
        const side = Math.min(img.width, img.height);
        ctx.drawImage(img, (img.width - side) / 2, (img.height - side) / 2, side, side, 0, 0, 128, 128);
        try {
          await invoke('set_custom_icon', { dataUrl: canvas.toDataURL('image/png') });
          await reload();
        } catch (e) {
          toast(String(e));
        }
      };
      img.src = reader.result;
    };
    reader.readAsDataURL(file);
  };
  input.click();
}

$('btn-icon-pick').addEventListener('click', pickLogoImage);

$('seg-logo-style').addEventListener('click', async (e) => {
  // 选项是图片按钮：点击可能落在内部 img/span 上，需上溯到 button
  const btn = e.target.closest?.('button[data-v]');
  const v = btn?.dataset.v;
  if (!v) return;
  try {
    if (v === 'custom' && !state.settings?.custom_icon) {
      pickLogoImage(); // 尚无自定义图片：选图成功后经 set_custom_icon 落为 custom
    } else {
      await invoke('set_logo_style', { style: v });
      await reload();
    }
  } catch (err) {
    toast(String(err));
  }
});

// ─── 更新（侧边 logo 行小按钮 + 关于页手动检查） ──────────────
let updateUrl = null;
let updateAsset = null;
let downloading = false;
function showUpdateSide(info) {
  if (info && info.has_update && info.url) {
    updateUrl = info.url;
    updateAsset = info.asset_url || null;
    $('btn-update-side').hidden = false;
  }
}
// 应用内直接下载当前平台安装包，按钮实时显示百分比（与弹窗按钮同款链路）
async function startSideDownload() {
  if (downloading || !updateAsset) return;
  downloading = true;
  const btn = $('btn-update-side');
  btn.disabled = true;
  const original = btn.innerHTML;
  btn.textContent = '0%';
  const un = await listen('update-download-progress', (e) => {
    btn.textContent = `${e.payload}%`;
  });
  try {
    // Windows：下载完成后后端自动启动安装器并退出应用；macOS/Linux 打开所在文件夹
    await invoke('download_and_install', { url: updateAsset });
  } catch (e) {
    toast(String(e));
  } finally {
    un();
    downloading = false;
    btn.disabled = false;
    btn.innerHTML = original;
  }
}
$('btn-update-side').addEventListener('click', () => {
  if (downloading) return;
  if (updateAsset) startSideDownload();
  else if (updateUrl) invoke('open_external', { url: updateUrl }).catch(() => {});
});
$('btn-check-update').addEventListener('click', async () => {
  const btn = $('btn-check-update');
  if (btn.disabled) return;
  const original = btn.textContent;
  btn.disabled = true;
  btn.textContent = '检查中…';
  try {
    const info = await invoke('check_update');
    if (info.error) {
      toast(`检查更新失败：${info.error}`);
    } else if (info.has_update) {
      showUpdateSide(info);
      toast(info.asset_url
        ? `发现新版本 v${info.latest}，正在下载安装包…`
        : `发现新版本 v${info.latest}，点击侧边按钮前往下载`);
      if (info.asset_url) startSideDownload();
    } else {
      // 已是最新：toast 明确反馈当前版本
      toast(`已是最新版本 v${info.current}`);
    }
  } catch (e) {
    toast(String(e));
  }
  btn.disabled = false;
  btn.textContent = original;
});

$('btn-repo').addEventListener('click', () => {
  invoke('open_external', { url: 'https://github.com/zander-zyx/coding-plan-limit' }).catch(() => {});
});

// ─── 数据加载与初始化 ─────────────────────────────────────────
async function reload() {
  try {
    state.views = await invoke('get_views');
    state.settings = await invoke('get_settings');
    renderPlans();
    renderSettings();
    renderSideLogo();
  } catch (e) {
    // 异常显形：不静默吞掉
    const box = $('plan-list');
    if (box) box.innerHTML = `<div class="p-empty" style="color:var(--urgent)">渲染异常：${esc(String(e && e.message || e))}</div>`;
    console.error('reload failed:', e);
  }
}

function renderSideLogo() {
  const logo = $('logo');
  const dot = $('logo-dot');
  const style = state.settings?.logo_style;
  // custom 图与 mark 为图片标：替换 brand-dot 显示；custom_icon 仅在 custom 样式下使用
  const customImg = style === 'custom' && state.settings?.custom_icon;
  const builtinImg = !customImg && style === 'mark';
  let img = logo.querySelector('img.side-logo-img');
  if (customImg || builtinImg) {
    dot.style.display = 'none';
    if (!img) {
      img = document.createElement('img');
      img.className = 'side-logo-img';
      logo.prepend(img);
    }
    img.src = customImg || 'icons/app-mark.png';
  } else {
    dot.style.display = '';
    img?.remove();
  }
}

(async () => {
  await initTheme();
  state.templates = await invoke('list_templates');
  try {
    $('config-dir').textContent = await invoke('get_config_dir');
  } catch { /* 忽略 */ }
  await reload();

  await listen('views-updated', async (e) => {
    // 拖拽进行中跳过重渲染（重建 DOM 会中断 drop）
    if (document.querySelector('#plan-list .row.dragging')) return;
    state.views = e.payload || [];
    renderPlans();
  });
  await listen('settings-updated', (e) => {
    if (e.payload) {
      state.settings = e.payload;
      applySettingsLook(e.payload);
      renderSettings();
      renderSideLogo();
    }
  });

  invoke('get_update_info').then(showUpdateSide).catch(() => {});
  await listen('update-available', (e) => showUpdateSide(e.payload));

  try {
    const v = await window.__TAURI__.app.getVersion();
    $('about-version').textContent = `v${v}`;
    $('side-version').textContent = v;
  } catch { /* 保持占位 */ }
})();

// ─── Codex 多账号（捕获副本 / 托管登录） ──────────────────────────────────
let codexPollTimer = null;
let codexLogin = null; // { deviceCode, userCode }

async function refreshCodexAccounts() {
  const sel = $('f-codex-account');
  try {
    const accounts = await invoke('codex_accounts');
    sel.innerHTML = '<option value="" disabled selected>选择已登录账号…</option>' +
      accounts.map((a) => {
        const label = a.email || `${a.account_id.slice(0, 14)}…`;
        return `<option value="${esc(a.account_id)}">${esc(label)}</option>`;
      }).join('');
  } catch {
    sel.innerHTML = '<option value="">账号列表加载失败</option>';
  }
}

function resetCodexLoginBox() {
  if (codexPollTimer) { clearInterval(codexPollTimer); codexPollTimer = null; }
  codexLogin = null;
  $('codex-login-box').hidden = true;
  $('codex-login-status').classList.remove('error');
}

function codexLoginFail(msg) {
  // 停轮询但保留面板：错误必须可见，否则用户无从得知失败原因
  if (codexPollTimer) { clearInterval(codexPollTimer); codexPollTimer = null; }
  const st = $('codex-login-status');
  st.textContent = msg;
  st.classList.add('error');
}

$('btn-codex-capture').addEventListener('click', async () => {
  if (!state.editingId) return;
  try {
    const warning = await invoke('codex_capture_for_plan', { planId: state.editingId });
    $('f-codex-hint').textContent = warning
      ? `已捕获，但${warning}`
      : '已捕获当前登录为本套餐副本（不自动刷新，凭据过期后需重新捕获）。';
  } catch (e) {
    toast(String(e));
  }
});

$('btn-codex-login').addEventListener('click', async () => {
  try {
    const s = await invoke('codex_login_start', {});
    codexLogin = { deviceCode: s.device_code, userCode: s.user_code };
    $('codex-user-code').textContent = s.user_code;
    $('codex-login-status').textContent = '等待授权…';
    $('codex-login-box').hidden = false;
    if (codexPollTimer) clearInterval(codexPollTimer);
    codexPollTimer = setInterval(async () => {
      try {
        const r = await invoke('codex_login_poll', {
          deviceCode: codexLogin.deviceCode,
          userCode: codexLogin.userCode,
        });
        if (r.status === 'done') {
          resetCodexLoginBox();
          await refreshCodexAccounts();
          $('f-codex-hint').textContent = '登录成功，可在上方下拉中选择并绑定该账号。';
        } else if (r.status === 'error') {
          codexLoginFail(r.error || '登录失败，请重试');
        }
      } catch (e) {
        codexLoginFail(String(e));
      }
    }, 2500);
  } catch (e) {
    toast(String(e));
  }
});

$('btn-codex-open').addEventListener('click', () => {
  invoke('open_external', { url: 'https://auth.openai.com/codex/device' }).catch(() => {});
});

$('btn-codex-copy').addEventListener('click', () => {
  if (codexLogin) navigator.clipboard?.writeText(codexLogin.userCode).catch(() => {});
});

$('btn-codex-bind').addEventListener('click', async () => {
  const id = $('f-codex-account').value;
  if (!id) return;
  if (!state.editingId) {
    $('f-codex-hint').textContent = '先保存套餐，再回来绑定账号。';
    return;
  }
  try {
    await invoke('codex_bind_plan', { planId: state.editingId, accountId: id });
    $('f-codex-hint').textContent = '已绑定所选托管账号。';
  } catch (e) {
    toast(String(e));
  }
});

$('btn-codex-follow').addEventListener('click', async () => {
  if (!state.editingId) return;
  try {
    await invoke('codex_bind_plan', { planId: state.editingId, accountId: null });
    $('f-codex-hint').textContent = '已恢复跟随本机当前登录。';
  } catch (e) {
    toast(String(e));
  }
});
