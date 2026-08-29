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
  accentPresets: ['#7c5cff', '#3f7cff', '#16c8b7', '#f59e0b', '#fb7185', '#0ea5a3'],
};

// ─── 视图切换 ─────────────────────────────────────────────────
document.querySelectorAll('.side nav button').forEach((btn) => {
  btn.addEventListener('click', () => {
    document.querySelectorAll('.side nav button').forEach((b) => b.classList.remove('active'));
    btn.classList.add('active');
    ['plans', 'settings', 'about'].forEach((v) => {
      $(`view-${v}`).hidden = v !== btn.dataset.view;
    });
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
        if (el.dataset.armed) { removePlan(el.dataset.del); return; }
        el.dataset.armed = '1';
        el.textContent = '确认删除';
        setTimeout(() => { el.dataset.armed = ''; el.textContent = '删除'; }, 2500);
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
  codex: 'Codex', 'claude-cache': 'Claude 缓存', xiaomi: '小米', deepseek: 'DeepSeek',
  kimi: 'Kimi', stepfun: '阶跃', siliconflow: '硅基流动', alibaba: '阿里云',
  packycode: 'PackyCode', newapi: 'NewAPI', sub2api: 'Sub2API',
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

  $('f-region-item').hidden = !t.has_region;
  $('f-kimi-variant-item').hidden = state.selectedTpl !== 'kimi';
  $('seg-kimi-variant').querySelectorAll('button').forEach((b) =>
    b.classList.toggle('active', b.dataset.v === state.tplVariant));
  $('f-bearer-item').hidden = t.auth !== 'bearer';
  $('f-cookie-item').hidden = t.auth !== 'cookie';
  $('f-cookie-key-item').hidden = t.auth !== 'cookie';
  $('f-ak-item').hidden = t.auth !== 'bss';
  $('f-ak-secret-item').hidden = t.auth !== 'bss';
  $('f-baseurl-item').hidden = !t.needs_base_url;
  $('f-baseurl-hint').textContent = t.id === 'zhipu'
    ? '默认 https://open.bigmodel.cn/api，国际站 https://api.z.ai/api'
    : t.id === 'packycode'
      ? '默认 https://www.packyapi.ai，其他中转站填对应地址'
      : '必填，例如 https://your-newapi-site.com';

  $('f-threshold-label').textContent =
    t.quota_type === 'balance' ? '余额提醒下限' : '提醒阈值（剩余 %）';
  $('f-threshold-hint').textContent =
    t.quota_type === 'balance'
      ? '账户余额低于该数值时触发系统通知'
      : '任一窗口剩余百分比低于该值时触发系统通知';

  if (editing) {
    $('f-name').value = editing.plan.name;
    $('f-region').value = editing.plan.region || 'cn';
    $('f-baseurl').value = editing.plan.base_url || '';
    $('f-threshold').value = editing.plan.threshold;
    $('f-enabled').checked = editing.plan.enabled;
    const ph = '已配置，留空保持不变';
    $('f-bearer').placeholder = ph;
    $('f-cookie').placeholder = ph;
    $('f-cookie-key').placeholder = ph;
    $('f-ak-id').placeholder = ph;
    $('f-ak-secret').placeholder = ph;
  } else {
    $('f-name').value = shortLabel(t.id);
    $('f-region').value = 'cn';
    $('f-baseurl').value = '';
    $('f-threshold').value = 10;
    $('f-enabled').checked = true;
    ['f-bearer', 'f-cookie', 'f-cookie-key', 'f-ak-id', 'f-ak-secret'].forEach((id) => ($(id).placeholder = ''));
  }
  renderLogoPreview();
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
}

function closeModal() {
  $('plan-modal').classList.remove('show');
  ['f-name', 'f-bearer', 'f-cookie', 'f-cookie-key', 'f-ak-id', 'f-ak-secret', 'f-baseurl']
    .forEach((id) => ($(id).value = ''));
  state.editingId = null;
  state.selectedTpl = null;
  state.tplVariant = 'kimi';
  state.pendingLogo = null;
}

$('btn-add').addEventListener('click', () => openModal(null));
$('btn-cancel').addEventListener('click', closeModal);
$('plan-modal').addEventListener('click', (e) => {
  if (e.target === $('plan-modal')) closeModal();
});
$('f-name').addEventListener('input', validateForm);

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
    threshold: parseFloat($('f-threshold').value) || 0,
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

$('btn-delete').addEventListener('click', async () => {
  if (!state.editingId) return;
  // 两步确认，与列表行删除一致
  const b = $('btn-delete');
  if (!b.dataset.armed) {
    b.dataset.armed = '1';
    b.textContent = '确认删除';
    setTimeout(() => { b.dataset.armed = ''; b.textContent = '删除'; }, 2500);
    return;
  }
  b.dataset.armed = '';
  b.textContent = '删除';
  const id = state.editingId;
  closeModal();
  await removePlan(id);
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
    b.addEventListener('click', () => saveSettings({ accent: b.dataset.c })));
  $('accent-picker').value = s.accent || '#7c5cff';
  $('accent-hex').value = (s.accent || '#7c5cff').toUpperCase();

  const mode = s.notify_mode || 'interval';
  $('seg-notify').querySelectorAll('button').forEach((b) =>
    b.classList.toggle('active', b.dataset.v === mode));
  $('row-notify-interval').hidden = mode !== 'interval';
  $('row-notify-count').hidden = mode !== 'count';
  $('in-notify-interval').value = s.notify_interval_minutes ?? 60;
  $('in-notify-count').value = s.notify_count ?? 10;

  $('in-refresh').value = s.refresh_seconds ?? 30;
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

  const preview = $('icon-preview');
  if (s.custom_icon) {
    preview.src = s.custom_icon;
    preview.hidden = false;
    $('btn-icon-reset').hidden = false;
  } else {
    preview.hidden = true;
    $('btn-icon-reset').hidden = true;
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
    if (patch.accent !== undefined) applyAccent(patch.accent);
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
$('in-autostart').addEventListener('change', (e) =>
  saveSettings({ autostart: e.target.checked }));
$('in-auto-update').addEventListener('change', (e) =>
  saveSettings({ auto_check_update: e.target.checked }));

$('accent-picker').addEventListener('change', (e) => {
  const v = e.target.value;
  if (/^#[0-9a-fA-F]{6}$/.test(v)) {
    $('accent-hex').value = v.toUpperCase();
    saveSettings({ accent: v });
  }
});
$('accent-hex').addEventListener('change', (e) => {
  const v = e.target.value.trim();
  if (/^#[0-9a-fA-F]{6}$/.test(v)) {
    saveSettings({ accent: v }).then(renderSettings);
  } else {
    toast('颜色格式应为 #RRGGBB');
    e.target.value = (state.settings.accent || '#7C5CFF').toUpperCase();
  }
});
$('accent-reset').addEventListener('click', () => saveSettings({ accent: null }).then(renderSettings));

// ─── 托盘图标自定义 ───────────────────────────────────────────
$('btn-icon-pick').addEventListener('click', () => {
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
});

$('btn-icon-reset').addEventListener('click', async () => {
  try {
    await invoke('reset_custom_icon');
    await reload();
  } catch (e) {
    toast(String(e));
  }
});

// ─── 更新（侧边 logo 行小按钮 + 关于页手动检查） ──────────────
let updateUrl = null;
function showUpdateSide(info) {
  if (info && info.has_update && info.url) {
    updateUrl = info.url;
    $('btn-update-side').hidden = false;
  }
}
$('btn-update-side').addEventListener('click', () => {
  if (updateUrl) invoke('open_external', { url: updateUrl }).catch(() => {});
});
$('btn-check-update').addEventListener('click', async () => {
  try {
    const info = await invoke('check_update');
    if (info.error) {
      toast(`检查更新失败：${info.error}`);
    } else if (info.has_update) {
      showUpdateSide(info);
      invoke('open_external', { url: info.url }).catch(() => {});
    }
  } catch (e) {
    toast(String(e));
  }
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
  const custom = state.settings?.custom_icon;
  let img = logo.querySelector('img.side-logo-img');
  if (custom) {
    dot.style.display = 'none';
    if (!img) {
      img = document.createElement('img');
      img.className = 'side-logo-img';
      logo.prepend(img);
    }
    img.src = custom;
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
