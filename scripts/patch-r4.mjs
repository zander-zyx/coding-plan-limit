// 第四轮 review 修复批：L6 阈值感知紧急、L10 hex 容错与默认回退、L9 表单保持、M8 前端阈值夹紧
import fs from 'node:fs';

// ── common.js ──
{
  let c = fs.readFileSync('ui/js/common.js', 'utf8');
  const before = c;

  // L6: 紧急判定感知用户阈值（windows 分支）
  c = c.replace(
    `      urgent = isUrgent(worst);
      // 每个窗口一行`,
    `      urgent = isUrgent(worst)
        || wins.some((w) => 100 - w.used_percent <= (view.plan.threshold ?? 0));
      // 每个窗口一行`,
  );
  c = c.replace(
    `        const urgentLine = isUrgent(u) ? 'urgent' : '';`,
    `        const urgentLine = isUrgent(u) || (100 - u <= (view.plan.threshold ?? 0)) ? 'urgent' : '';`,
  );

  // L10: applyAccent 非法/为空回默认 hue 252
  c = c.replace(
    `function applyAccent(hex) {
  if (!hex || !/^#[0-9a-fA-F]{6}$/.test(hex)) return;
  document.documentElement.style.setProperty('--brand-hue', String(hexToHue(hex)));
}`,
    `function applyAccent(hex) {
  const ok = hex && /^#[0-9a-fA-F]{6}$/.test(hex);
  document.documentElement.style.setProperty('--brand-hue', ok ? String(hexToHue(hex)) : '252');
}`,
  );
  c = c.replace(
    `  if (s.accent) applyAccent(s.accent);
  else document.documentElement.style.setProperty('--brand-hue', '252');`,
    `  applyAccent(s.accent);`,
  );

  fs.writeFileSync('ui/js/common.js', c);
  console.log('common.js changed:', c !== before);
}

// ── app.js ──
{
  let a = fs.readFileSync('ui/js/app.js', 'utf8');
  const before = a;

  // M8 前端阈值夹紧（后端为权威）
  a = a.replace(
    `    threshold: parseFloat($('f-threshold').value) || 0,`,
    `    threshold: (() => {
      const raw = parseFloat($('f-threshold').value);
      if (!Number.isFinite(raw)) return t.quota_type === 'balance' ? 0 : 10;
      return t.quota_type === 'balance' ? Math.max(0, raw) : Math.min(100, Math.max(0, raw));
    })(),`,
  );

  // L9: 模板/Kimi 变体切换时保留未保存的名称/阈值/地址输入
  a = a.replace(
    `function renderFormFields() {
  const t = currentTpl();
  if (!t) return;
  const editing = state.editingId && state.views.find((v) => v.plan.id === state.editingId);
  state.pendingLogo = editing ? (editing.plan.logo || null) : null;`,
    `function renderFormFields() {
  const t = currentTpl();
  if (!t) return;
  const editing = state.editingId && state.views.find((v) => v.plan.id === state.editingId);
  state.pendingLogo = editing ? (editing.plan.logo || null) : null;
  // 切换模板/变体时保留用户已输入但未保存的内容
  const keepName = $('f-name').value;
  const keepThreshold = $('f-threshold').value;
  const keepBaseUrl = $('f-baseurl').value;`,
  );
  a = a.replace(
    `  if (editing) {
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
  renderLogoPreview();`,
    `  if (editing) {
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
    $('f-name').value = keepName || shortLabel(t.id);
    $('f-region').value = 'cn';
    $('f-baseurl').value = keepBaseUrl;
    $('f-threshold').value = keepThreshold || 10;
    $('f-enabled').checked = true;
    ['f-bearer', 'f-cookie', 'f-cookie-key', 'f-ak-id', 'f-ak-secret'].forEach((id) => ($(id).placeholder = ''));
  }
  renderLogoPreview();`,
  );

  // L10: hex 输入自动补 #
  a = a.replace(
    `$('accent-hex').addEventListener('change', (e) => {
  const v = e.target.value.trim();`,
    `$('accent-hex').addEventListener('change', (e) => {
  const v = e.target.value.trim().replace(/^#?([0-9a-fA-F]{6})$/, '#$1');`,
  );

  fs.writeFileSync('ui/js/app.js', a);
  console.log('app.js changed:', a !== before);
}

// ── mod.rs：newapi 系描述注明币种跟随站点后台 ──
{
  let m = fs.readFileSync('src-tauri/src/usage/mod.rs', 'utf8');
  const before = m;
  m = m.replace(
    `description: "余额查询（OpenAI 兼容计费接口）".into(),`,
    `description: "余额（币种跟随站点后台显示设置）".into(),`,
  );
  m = m.replace(
    `description: "需填写站点地址，走 OpenAI 兼容计费接口".into(),`,
    `description: "填站点地址；币种跟随站点后台设置".into(),`,
  );
  fs.writeFileSync('src-tauri/src/usage/mod.rs', m);
  console.log('mod.rs changed:', m !== before);
}
