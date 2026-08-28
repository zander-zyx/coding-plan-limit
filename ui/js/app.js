// 主窗口逻辑：仪表盘 / 套餐管理 / 设置（更改即时保存）

const $ = (id) => document.getElementById(id);

const state = {
  views: [],
  templates: [],
  settings: null,
  editingId: null,      // null=新增
  selectedTpl: null,
  accentPresets: ['#5b8cff', '#34d399', '#f59e0b', '#f87171', '#8b5cf6', '#0ea5a3'],
};

// ─── 视图切换 ─────────────────────────────────────────────────
document.querySelectorAll('.side nav button').forEach((btn) => {
  btn.addEventListener('click', () => {
    document.querySelectorAll('.side nav button').forEach((b) => b.classList.remove('active'));
    btn.classList.add('active');
    ['dash', 'plans', 'settings'].forEach((v) => {
      $(`view-${v}`).hidden = v !== btn.dataset.view;
    });
  });
});

// ─── 仪表盘 ───────────────────────────────────────────────────
function renderDash() {
  const enabled = state.views.filter((v) => v.plan.enabled);
  const bad = enabled.filter((v) => v.snapshot && !v.snapshot.ok).length;
  $('dash-sub').textContent = `共 ${state.views.length} 个套餐 · 启用 ${enabled.length} 个`;

  $('dash-stats').innerHTML = `
    <div class="stat">套餐总数<b>${state.views.length}</b></div>
    <div class="stat">已启用<b>${enabled.length}</b></div>
    <div class="stat">异常<b style="color:${bad ? 'var(--bad)' : 'var(--good)'}">${bad}</b></div>`;

  $('dash-grid').innerHTML = state.views.length
    ? state.views.map((v) => cardHtml(v, 'dash')).join('')
    : `<div class="empty" style="grid-column:1/-1;padding:60px 0">
         还没有套餐，去「套餐管理」添加第一个
       </div>`;
  animateCards($('dash-grid'));
}

$('btn-refresh').addEventListener('click', () => {
  invoke('refresh_now');
  toast('已触发刷新');
});

// ─── 套餐管理 ─────────────────────────────────────────────────
function renderPlans() {
  const box = $('plan-list');
  if (!state.views.length) {
    box.innerHTML = `<div class="empty" style="padding:60px 0">还没有套餐，点击右上角「添加套餐」</div>`;
    return;
  }
  box.innerHTML = state.views.map((v) => {
    const m = metaOf(v);
    const snap = v.snapshot;
    const status = !v.plan.enabled
      ? '已停用'
      : snap && snap.ok
        ? '正常'
        : snap
          ? `异常：${esc(snap.error || '')}`
          : '待刷新';
    return `
    <div class="plan-row ${v.plan.enabled ? '' : 'disabled'}">
      <label class="switch" title="启用/停用">
        <input type="checkbox" data-toggle="${v.plan.id}" ${v.plan.enabled ? 'checked' : ''} /><i></i>
      </label>
      <div class="info">
        <div class="name"><i style="width:8px;height:8px;border-radius:50%;background:${m.color}"></i>${esc(v.plan.name)}</div>
        <div class="meta">${esc(m.name)} · 提醒阈值 ${v.plan.threshold} · ${esc(status)}</div>
      </div>
      <div class="actions">
        <button data-edit="${v.plan.id}">编辑</button>
        <button data-del="${v.plan.id}">删除</button>
      </div>
    </div>`;
  }).join('');

  box.querySelectorAll('[data-toggle]').forEach((el) =>
    el.addEventListener('change', () => togglePlan(el.dataset.toggle, el.checked)));
  box.querySelectorAll('[data-edit]').forEach((el) =>
    el.addEventListener('click', () => openModal(el.dataset.edit)));
  box.querySelectorAll('[data-del]').forEach((el) =>
    el.addEventListener('click', () => removePlan(el.dataset.del)));
}

async function togglePlan(id, enabled) {
  const view = state.views.find((v) => v.plan.id === id);
  if (!view) return;
  try {
    await invoke('save_plan', { plan: { ...view.plan, enabled }, secret: null });
    toast(enabled ? '已启用' : '已停用');
  } catch (e) {
    toast(String(e), true);
  }
  await reload();
}

async function removePlan(id) {
  const view = state.views.find((v) => v.plan.id === id);
  if (!confirm(`确定删除套餐「${view?.plan.name}」？\n对应密钥也将一并从系统凭据库清除。`)) return;
  try {
    await invoke('delete_plan', { id });
    toast('已删除');
  } catch (e) {
    toast(String(e), true);
  }
  await reload();
}

// ─── 添加/编辑弹层 ────────────────────────────────────────────
function renderTplGrid() {
  $('tpl-grid').innerHTML = state.templates.map((t) => {
    const m = PROVIDER_META[t.id] || { color: '#8b93a7' };
    return `
    <button class="tpl-item ${state.selectedTpl === t.id ? 'active' : ''}" data-tpl="${t.id}">
      <div class="t-name"><i style="background:${m.color}"></i>${esc(t.name)}</div>
      <div class="t-desc">${esc(t.description)}</div>
    </button>`;
  }).join('');
  $('tpl-grid').querySelectorAll('[data-tpl]').forEach((el) =>
    el.addEventListener('click', () => {
      state.selectedTpl = el.dataset.tpl;
      renderTplGrid();
      renderFormFields();
      $('plan-form').hidden = false;
      validateForm();
    }));
}

function currentTpl() {
  return state.templates.find((t) => t.id === state.selectedTpl);
}

/** 按模板显示/隐藏表单字段 */
function renderFormFields() {
  const t = currentTpl();
  if (!t) return;
  const editing = state.editingId && state.views.find((v) => v.plan.id === state.editingId);

  $('f-region-item').hidden = !t.has_region;
  $('f-bearer-item').hidden = t.auth !== 'bearer';
  $('f-cookie-item').hidden = t.auth !== 'cookie';
  $('f-cookie-key-item').hidden = t.auth !== 'cookie';
  $('f-ak-item').hidden = t.auth !== 'bss';
  $('f-ak-secret-item').hidden = t.auth !== 'bss';
  $('f-baseurl-item').hidden = !t.needs_base_url;
  $('f-baseurl-hint').textContent = t.id === 'zhipu'
    ? '默认 https://open.bigmodel.cn/api，国际站 https://api.z.ai/api；使用代理时填完整基础地址'
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
    $('f-name').value = t.name;
    $('f-region').value = 'cn';
    $('f-baseurl').value = '';
    $('f-threshold').value = 10;
    $('f-enabled').checked = true;
    ['f-bearer', 'f-cookie', 'f-cookie-key', 'f-ak-id', 'f-ak-secret'].forEach((id) => ($(id).placeholder = ''));
  }
}

function validateForm() {
  $('btn-save').disabled = !state.selectedTpl || !$('f-name').value.trim();
}

function openModal(planId) {
  state.editingId = planId || null;
  state.selectedTpl = planId
    ? state.views.find((v) => v.plan.id === planId)?.plan.template
    : null;
  $('modal-title').textContent = planId ? '编辑套餐' : '添加套餐';
  $('btn-delete').hidden = !planId;
  $('plan-form').hidden = !planId; // 新增时先选模板
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
}

$('btn-add').addEventListener('click', () => openModal(null));
$('btn-cancel').addEventListener('click', closeModal);
$('plan-modal').addEventListener('click', (e) => {
  if (e.target === $('plan-modal')) closeModal();
});
$('f-name').addEventListener('input', validateForm);

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
    if (out.warning) toast(out.warning, true);
    else toast('已保存');
    closeModal();
  } catch (e) {
    toast(String(e), true);
  }
  await reload();
});

$('btn-delete').addEventListener('click', async () => {
  if (!state.editingId) return;
  const id = state.editingId;
  closeModal();
  await removePlan(id);
});

// ─── 设置 ─────────────────────────────────────────────────────
function renderSettings() {
  const s = state.settings;
  if (!s) return;

  // 主题
  $('seg-theme').querySelectorAll('button').forEach((b) =>
    b.classList.toggle('active', b.dataset.v === (s.theme || 'system')));

  // 主题色预设
  $('accent-presets').innerHTML = state.accentPresets
    .map((c) => `<button data-c="${c}" style="width:22px;height:22px;border-radius:6px;background:${c}" title="${c}"></button>`)
    .join('');
  $('accent-presets').querySelectorAll('[data-c]').forEach((b) =>
    b.addEventListener('click', () => saveSettings({ accent: b.dataset.c })));
  $('accent-picker').value = s.accent || '#5b8cff';

  // 通知
  const mode = s.notify_mode || 'interval';
  $('seg-notify').querySelectorAll('button').forEach((b) =>
    b.classList.toggle('active', b.dataset.v === mode));
  $('row-notify-interval').hidden = mode !== 'interval';
  $('row-notify-count').hidden = mode !== 'count';
  $('in-notify-interval').value = s.notify_interval_minutes ?? 60;
  $('in-notify-count').value = s.notify_count ?? 10;

  $('in-refresh').value = s.refresh_seconds ?? 30;
  $('in-autostart').checked = !!s.autostart;

  // 进度样式
  $('seg-barstyle').querySelectorAll('button').forEach((b) =>
    b.classList.toggle('active', b.dataset.v === (s.bar_style || 'bar')));

  // 主题色 hex 输入框回填
  $('accent-hex').value = (s.accent || '#5b8cff').toUpperCase();

  // 悬浮窗展示选择（最多 10，计数从当前勾选状态实时计算）
  const enabledViews = state.views.filter((v) => v.plan.enabled);
  const picked = new Set(s.popup_plan_ids || []);
  const rows = enabledViews.map((v) => {
    const m = metaOf(v);
    return `
    <label class="pick-row">
      <input type="checkbox" data-pick="${v.plan.id}" ${picked.has(v.plan.id) ? 'checked' : ''} />
      <span class="dot" style="background:${m.color}"></span>${esc(v.plan.name)}
    </label>`;
  }).join('');
  $('popup-pick').innerHTML = enabledViews.length
    ? rows + `<div class="pick-hint" id="pick-hint"></div>`
    : `<div class="pick-hint">暂无启用的套餐</div>`;
  const updatePickHint = () => {
    const n = $('popup-pick').querySelectorAll('[data-pick]:checked').length;
    const hint = $('pick-hint');
    if (hint) hint.textContent = `已选 ${Math.min(n, 10)}/10（未选的按添加顺序自动补足）`;
  };
  updatePickHint();
  $('popup-pick').querySelectorAll('[data-pick]').forEach((el) =>
    el.addEventListener('change', () => {
      let ids = [...$('popup-pick').querySelectorAll('[data-pick]:checked')]
        .map((x) => x.dataset.pick);
      if (ids.length > 10) {
        el.checked = false;
        ids = ids.slice(0, 10);
        toast('悬浮窗最多固定展示 10 家', true);
        updatePickHint();
        return;
      }
      updatePickHint();
      saveSettings({ popup_plan_ids: ids });
    }));

  // 托盘图标预览
  const preview = $('icon-preview');
  if (s.custom_icon) {
    preview.src = s.custom_icon;
    preview.hidden = false;
  } else {
    preview.hidden = true;
  }
}

/** 局部更新设置并持久化（其余字段沿用当前值） */
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
    if (patch.bar_style !== undefined) window.__barStyle = patch.bar_style === 'ring' ? 'ring' : 'bar';
  } catch (e) {
    toast(String(e), true);
  }
}

// 主题切换
$('seg-theme').addEventListener('click', (e) => {
  const v = e.target.dataset?.v;
  if (v) saveSettings({ theme: v }).then(renderSettings);
});

// 通知模式
$('seg-notify').addEventListener('click', (e) => {
  const v = e.target.dataset?.v;
  if (v) saveSettings({ notify_mode: v }).then(renderSettings);
});
$('in-notify-interval').addEventListener('change', (e) =>
  saveSettings({ notify_interval_minutes: Math.max(1, parseInt(e.target.value) || 60) }));
$('in-notify-count').addEventListener('change', (e) =>
  saveSettings({ notify_count: Math.max(1, parseInt(e.target.value) || 10) }));

// 刷新间隔 / 自启
$('in-refresh').addEventListener('change', (e) =>
  saveSettings({ refresh_seconds: Math.max(10, parseInt(e.target.value) || 30) }));
$('in-autostart').addEventListener('change', (e) =>
  saveSettings({ autostart: e.target.checked }));

// 进度样式切换
$('seg-barstyle').addEventListener('click', (e) => {
  const v = e.target.dataset?.v;
  if (v) saveSettings({ bar_style: v }).then(renderSettings);
});

// 主题色：拾色器 + #RRGGBB 输入 + 预设
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
    toast('颜色格式应为 #RRGGBB，例如 #5B8CFF', true);
    e.target.value = (state.settings.accent || '#5B8CFF').toUpperCase();
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
        // 统一缩放为 128×128 PNG dataURL 再交给 Rust
        const canvas = document.createElement('canvas');
        canvas.width = 128;
        canvas.height = 128;
        const ctx = canvas.getContext('2d');
        const side = Math.min(img.width, img.height);
        ctx.drawImage(
          img,
          (img.width - side) / 2, (img.height - side) / 2, side, side,
          0, 0, 128, 128,
        );
        const dataUrl = canvas.toDataURL('image/png');
        try {
          await invoke('set_custom_icon', { dataUrl });
          toast('托盘图标已更新');
          await reload();
        } catch (e) {
          toast(String(e), true);
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
    toast('已恢复默认图标');
    await reload();
  } catch (e) {
    toast(String(e), true);
  }
});

// ─── 数据加载与初始化 ─────────────────────────────────────────
async function reload() {
  state.views = await invoke('get_views');
  state.settings = await invoke('get_settings');
  renderDash();
  renderPlans();
  renderSettings();
  // 侧栏 logo 使用自定义图标（若有）
  if (state.settings.custom_icon && !$('logo').querySelector('img')) {
    const img = document.createElement('img');
    img.src = state.settings.custom_icon;
    $('logo').prepend(img);
    $('logo').firstChild.textContent = '';
  }
}

(async () => {
  await initTheme();
  state.templates = await invoke('list_templates');
  try {
    $('config-dir').textContent = await invoke('get_config_dir');
  } catch { /* 忽略 */ }
  await reload();

  await listen('views-updated', (e) => {
    state.views = e.payload || [];
    renderDash();
    renderPlans();
  });

  // 其他窗口（弹窗侧无写入口，主要防多实例场景）或后端广播的设置变更
  await listen('settings-updated', (e) => {
    if (e.payload) {
      state.settings = e.payload;
      applySettingsLook(e.payload);
      renderSettings();
    }
  });

  // GitHub 仓库入口
  $('btn-repo').addEventListener('click', () => {
    invoke('open_external', { url: 'https://github.com/zander-zyx/coding-plan-limit' })
      .catch((err) => toast(String(err), true));
  });
})();
