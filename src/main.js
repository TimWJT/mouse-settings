// Mouse Settings page: shows the profile read from the mouse, lets you edit it, saves it back.
// All device logic lives in Rust; this file only renders and collects edits.

import { api, isPreview } from './api.js';
import {
  SPECIALS, MOUSE_BUTTONS, MEDIA, KEY_USAGE, MOD, RATES, DPI,
  slotName, bindingLabel, rgbToHex, hexToRgb, equal, clone, snapDpi, summarizeChanges, modifierNames, USAGE_NAME,
} from './model.js';

const $ = id => document.getElementById(id);

const state = {
  info: null,
  address: null,
  original: null,
  edited: null,
  raw: [],
  readOnly: false,
  layer: 'buttons',
  busy: false,
};

const profileName = addr => (addr >= 0x100 ? `Factory profile ${addr - 0x100}` : `Profile ${addr}`);
const buttonCount = () => Math.min(state.info?.description?.buttonCount || 11, 16);
const dirty = () => state.edited && !equal(state.original, state.edited);

// ---------- small UI helpers ----------

let toastTimer;
function toast(message, kind = 'info') {
  const el = $('toast');
  el.textContent = message;
  el.dataset.kind = kind;
  el.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { el.hidden = true; }, kind === 'error' ? 9000 : 5000);
}

function ask({ title, items = [], note = '', ok }) {
  const dlg = $('confirm');
  $('confirm-h').textContent = title;
  $('confirm-list').replaceChildren(...items.map(t => Object.assign(document.createElement('li'), { textContent: t })));
  $('confirm-list').hidden = items.length === 0;
  $('confirm-note').textContent = note;
  $('confirm-ok').textContent = ok;
  dlg.returnValue = '';
  dlg.showModal();
  return new Promise(resolve => dlg.addEventListener('close', () => resolve(dlg.returnValue === 'ok'), { once: true }));
}

function el(tag, props = {}, ...children) {
  const node = Object.assign(document.createElement(tag), props);
  node.append(...children);
  return node;
}

const TRASH = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 6h18M8 6V4h8v2M19 6l-1 14H6L5 6"/></svg>';

async function busy(fn) {
  if (state.busy) return;
  state.busy = true;
  document.body.setAttribute('aria-busy', 'true');
  try {
    await fn();
  } catch (err) {
    toast(String(err), 'error');
  } finally {
    state.busy = false;
    document.body.removeAttribute('aria-busy');
  }
}

// ---------- loading ----------

async function refresh() {
  setDevice('wait', 'Looking for the mouse…');
  let info;
  try {
    info = await api.mouseInfo();
  } catch (err) {
    state.info = null;
    setDevice('off', 'Not connected');
    $('device-meta').textContent = '';
    showEmpty('Plug in your G502 HERO', String(err));
    renderRail();
    return;
  }
  state.info = info;
  setDevice('on', 'Connected');
  $('device-name').textContent = info.name || 'G502 HERO';
  $('device-meta').textContent = [info.firmware && `Firmware ${info.firmware}`, `HID++ ${info.protocol}`].filter(Boolean).join(' · ');
  $('host-notice').hidden = info.mode !== 'host';

  const readable = info.slots.filter(s => s.enabled);
  const keep = readable.find(s => s.address === state.address);
  const current = readable.find(s => s.address === info.currentProfile);
  const pick = keep || current || readable[0];
  renderRail();
  await loadBackups();
  if (!pick) {
    showEmpty('No profiles on the mouse', 'Set the mouse up once in Logitech G Hub so it has a profile to edit.');
    return;
  }
  await loadProfile(pick.address);
}

async function loadProfile(address) {
  const view = await api.readProfile(address);
  const slot = state.info.slots.find(s => s.address === address);
  state.address = address;
  state.original = view.settings;
  state.edited = clone(view.settings);
  state.raw = view.raw || [];
  state.readOnly = !slot?.writable || !view.crcOk;
  $('readonly-notice').textContent = !slot?.writable
    ? 'This is a factory profile. You can look at it, but it can’t be changed.'
    : 'This profile fails its checksum on the mouse, so it isn’t safe to edit. Restore a backup or set it up again in G Hub.';
  renderRail();
  renderEditor();
}

async function loadBackups() {
  const list = await api.listBackups();
  $('backups-empty').hidden = list.length > 0;
  $('backup-list').replaceChildren(...list.slice(0, 12).map(b => {
    const when = new Date(b.savedAt * 1000).toLocaleString(undefined, { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });
    const btn = el('button', { type: 'button', className: 'btn small', textContent: 'Restore' });
    btn.addEventListener('click', () => restoreBackup(b));
    return el('li', { className: 'backup-item' }, el('div', {}, `${profileName(b.address)} `, el('span', { textContent: when })), btn);
  }));
}

function setDevice(kind, text) {
  $('device-state').dataset.state = kind;
  $('device-state-text').textContent = text;
}

function showEmpty(title, text) {
  $('empty-h').textContent = title;
  $('empty-p').textContent = text;
  $('empty-state').hidden = false;
  $('editor').hidden = true;
  $('savebar').hidden = true;
  $('readonly-notice').hidden = true;
}

// ---------- rail ----------

function renderRail() {
  const slots = state.info?.slots ?? [];
  $('profile-list').replaceChildren(...slots.map(s => {
    const tag = !s.enabled ? 'off' : !s.writable ? 'read-only' : s.address === state.info.currentProfile ? 'in use' : '';
    const btn = el('button', { type: 'button', className: 'profile-btn', disabled: !s.enabled },
      el('span', { textContent: profileName(s.address) }), el('small', { textContent: tag }));
    btn.setAttribute('aria-current', String(s.address === state.address));
    btn.addEventListener('click', () => switchProfile(s.address));
    return el('li', {}, btn);
  }));
}

async function switchProfile(address) {
  if (address === state.address) return;
  if (dirty() && !(await ask({ title: 'Throw away your changes?', note: 'They haven’t been saved to the mouse.', ok: 'Throw away changes' }))) return;
  busy(() => loadProfile(address));
}

// ---------- editor ----------

function renderEditor() {
  const s = state.edited;
  $('empty-state').hidden = true;
  $('editor').hidden = false;
  $('readonly-notice').hidden = !state.readOnly;
  $('profile-title').textContent = profileName(state.address);
  $('profile-sub').textContent = s.name && s.name !== 'PROFILE_NAME_DEFAULT' ? s.name : '';
  renderStages();
  renderRate();
  renderBindings();
  renderZones();
  renderRaw();
  for (const input of $('editor').querySelectorAll('input, select, button')) {
    if (input.closest('.raw') || input.closest('[role="tablist"]')) continue;
    if (state.readOnly) input.disabled = true;
  }
  renderSaveBar();
}

function renderStages() {
  const s = state.edited;
  $('stages').replaceChildren(...s.dpiStages.map((dpi, i) => {
    const input = el('input', { type: 'number', min: DPI.min, max: DPI.max, step: DPI.step, value: dpi });
    input.setAttribute('aria-label', `Stage ${i + 1} DPI`);
    input.addEventListener('change', () => {
      s.dpiStages[i] = snapDpi(input.value);
      renderEditor();
    });
    const del = el('button', { type: 'button', className: 'icon-btn', innerHTML: TRASH, hidden: s.dpiStages.length < 2 });
    del.setAttribute('aria-label', `Remove stage ${i + 1}`);
    del.addEventListener('click', () => removeStage(i));
    const tags = [i === s.defaultStage && 'Starts here', i === s.shiftStage && 'Sniper'].filter(Boolean).join(' · ');
    const card = el('div', { className: 'stage' },
      el('div', { className: 'stage-top' }, el('span', { textContent: `Stage ${i + 1}` }), del),
      input,
      el('div', { className: 'stage-tags', textContent: tags }));
    card.dataset.default = String(i === s.defaultStage);
    return card;
  }));
  $('btn-add-stage').disabled = s.dpiStages.length >= 5;

  for (const [id, key] of [['default-stage', 'defaultStage'], ['shift-stage', 'shiftStage']]) {
    const sel = $(id);
    sel.replaceChildren(...s.dpiStages.map((dpi, i) => el('option', { value: i, textContent: `Stage ${i + 1} · ${dpi} DPI` })));
    sel.value = String(s[key]);
    sel.disabled = false; // static control: renderEditor re-disables it for read-only profiles
  }
}

function removeStage(i) {
  const s = state.edited;
  s.dpiStages.splice(i, 1);
  const fix = idx => (idx > i ? idx - 1 : Math.min(idx, s.dpiStages.length - 1));
  s.defaultStage = fix(s.defaultStage);
  s.shiftStage = fix(s.shiftStage);
  renderEditor();
}

function renderRate() {
  const s = state.edited;
  $('rate-seg').replaceChildren(...RATES.map(hz => {
    const b = el('button', { type: 'button', className: 'seg-btn', textContent: `${hz} Hz` });
    b.setAttribute('role', 'radio');
    b.setAttribute('aria-checked', String(s.reportRateHz === hz));
    b.addEventListener('click', () => { s.reportRateHz = hz; renderEditor(); });
    return b;
  }));
}

// Binding <select> values: "mouse:1", "special:3", "media:205", "disabled", "keys", "cur", "orig".
const CATALOG = [
  ['Mouse buttons', Object.entries(MOUSE_BUTTONS).map(([n, label]) => [`mouse:${n}`, label])],
  ['DPI', [3, 4, 5, 7, 6].map(a => [`special:${a}`, SPECIALS[a]])],
  ['Scroll', [1, 2].map(a => [`special:${a}`, SPECIALS[a]])],
  ['Profiles and G-Shift', [8, 9, 10, 11].map(a => [`special:${a}`, SPECIALS[a]])],
  ['Media keys', Object.entries(MEDIA).map(([u, label]) => [`media:${u}`, label])],
  ['Other', [['keys', 'Keyboard shortcut…'], ['disabled', 'Does nothing']]],
];
const CATALOG_VALUES = new Set(CATALOG.flatMap(([, opts]) => opts.map(([v]) => v)));

function catalogValue(b) {
  const v = b.kind === 'mouse' ? `mouse:${b.button}`
    : b.kind === 'special' ? `special:${b.action}`
    : b.kind === 'media' ? `media:${b.usage}`
    : b.kind === 'disabled' ? 'disabled' : null;
  return v && CATALOG_VALUES.has(v) ? v : null;
}

function bindingFromValue(v) {
  const [kind, n] = v.split(':');
  if (kind === 'mouse') return { kind, button: Number(n) };
  if (kind === 'special') return { kind, action: Number(n) };
  if (kind === 'media') return { kind, usage: Number(n) };
  return { kind: 'disabled' };
}

function renderBindings() {
  const list = state.edited[state.layer];
  const orig = state.original[state.layer];
  $('tab-normal').setAttribute('aria-selected', String(state.layer === 'buttons'));
  $('tab-gshift').setAttribute('aria-selected', String(state.layer === 'gshiftButtons'));

  const rows = [];
  for (let i = 0; i < buttonCount(); i++) {
    const cur = list[i];
    const sel = el('select');
    sel.setAttribute('aria-label', `${slotName(i)} action`);
    const curValue = catalogValue(cur);
    if (!curValue) sel.append(el('option', { value: 'cur', textContent: bindingLabel(cur) }));
    if (!catalogValue(orig[i]) && !equal(orig[i], cur)) {
      sel.append(el('option', { value: 'orig', textContent: `${bindingLabel(orig[i])} (as before)` }));
    }
    for (const [group, opts] of CATALOG) {
      sel.append(el('optgroup', { label: group }, ...opts.map(([value, label]) => el('option', { value, textContent: label }))));
    }
    sel.value = curValue ?? 'cur';
    sel.addEventListener('change', async () => {
      if (sel.value === 'cur') return;
      if (sel.value === 'orig') list[i] = clone(orig[i]);
      else if (sel.value === 'keys') {
        const key = await captureKeys();
        if (key) list[i] = key;
      } else list[i] = bindingFromValue(sel.value);
      renderEditor();
    });
    const row = el('li', { className: 'binding' },
      el('span', { className: 'binding-name', textContent: slotName(i) }), sel, el('span'));
    row.classList.toggle('binding-changed', !equal(orig[i], cur));
    rows.push(row);
  }
  $('bindings').replaceChildren(...rows);
}

function captureKeys() {
  const dlg = $('keycap');
  const preview = $('keycap-preview');
  const ok = $('keycap-ok');
  let picked = null;
  preview.textContent = 'Waiting…';
  ok.disabled = true;
  const onKey = e => {
    if (e.code === 'Escape' && !e.ctrlKey && !e.shiftKey && !e.altKey && !e.metaKey) return; // cancels
    e.preventDefault();
    const modifiers = (e.ctrlKey ? MOD.ctrl : 0) | (e.shiftKey ? MOD.shift : 0) | (e.altKey ? MOD.alt : 0) | (e.metaKey ? MOD.win : 0);
    const usage = KEY_USAGE[e.code];
    if (!usage) {
      const mods = modifierNames(modifiers);
      preview.textContent = mods.length ? `${mods.join(' + ')} + …` : 'That key can’t be assigned';
      return;
    }
    picked = { kind: 'key', modifiers, key: usage };
    preview.textContent = [...modifierNames(modifiers), USAGE_NAME[usage]].join(' + ');
    ok.disabled = false;
  };
  dlg.addEventListener('keydown', onKey);
  dlg.returnValue = '';
  dlg.showModal();
  return new Promise(resolve => dlg.addEventListener('close', () => {
    dlg.removeEventListener('keydown', onKey);
    resolve(dlg.returnValue === 'ok' ? picked : null);
  }, { once: true }));
}

const EFFECTS = [['off', 'Off'], ['fixed', 'Colour'], ['breathing', 'Breathing'], ['cycle', 'Colour cycle']];

function defaultsFor(kind, prev) {
  const color = prev.color ?? [255, 102, 0];
  if (kind === 'fixed') return { kind, color };
  if (kind === 'breathing') return { kind, color, periodMs: prev.periodMs ?? 4000, brightness: prev.brightness ?? 100 };
  if (kind === 'cycle') return { kind, periodMs: prev.periodMs ?? 10000, brightness: prev.brightness ?? 100 };
  return { kind: 'off' };
}

function renderZones() {
  const zones = state.edited.lighting;
  $('zones').replaceChildren(...zones.map((z, i) => {
    const orig = state.original.lighting[i];
    const seg = el('div', { className: 'seg small' });
    seg.setAttribute('role', 'radiogroup');
    seg.setAttribute('aria-label', `Zone ${i + 1} effect`);
    const choices = [...EFFECTS];
    if (orig.kind === 'raw') choices.push(['raw', 'Keep as is']);
    for (const [kind, label] of choices) {
      const b = el('button', { type: 'button', className: 'seg-btn', textContent: label });
      b.setAttribute('role', 'radio');
      b.setAttribute('aria-checked', String(z.kind === kind));
      b.addEventListener('click', () => {
        zones[i] = kind === 'raw' ? clone(orig) : z.kind === kind ? z : defaultsFor(kind, z);
        renderEditor();
      });
      seg.append(b);
    }

    const swatch = el('span', { className: 'swatch' });
    swatch.style.background = z.color ? rgbToHex(z.color) : z.kind === 'cycle' ? 'conic-gradient(red, yellow, lime, cyan, blue, magenta, red)' : 'transparent';
    const body = [el('div', { className: 'zone-head' }, el('h3', { textContent: `Lighting zone ${i + 1}`, className: 'binding-name' }), swatch), seg];

    if (z.kind === 'raw') body.push(el('p', { className: 'hint', textContent: 'An effect this app doesn’t know. It stays exactly as it is unless you pick another one.' }));
    const row = el('div', { className: 'zone-row' });
    if (z.color) {
      const pick = el('input', { type: 'color', value: rgbToHex(z.color).toLowerCase() });
      pick.setAttribute('aria-label', `Zone ${i + 1} colour`);
      pick.addEventListener('input', () => { z.color = hexToRgb(pick.value); swatch.style.background = pick.value; renderSaveBar(); });
      pick.addEventListener('change', renderEditor);
      row.append(el('label', { className: 'field' }, el('span', { textContent: 'Colour' }), pick));
    }
    if ('brightness' in z) {
      row.append(slider('Brightness', 1, 100, 1, z.brightness, v => `${v}%`, v => { z.brightness = v; }));
      row.append(slider('One cycle takes', 1, 20, 1, Math.round(z.periodMs / 1000), v => `${v} s`, v => { z.periodMs = v * 1000; }));
    }
    if (row.childElementCount) body.push(row);
    return el('div', { className: 'zone' }, ...body);
  }));
}

function slider(label, min, max, step, value, fmt, set) {
  const out = el('span', { className: 'value', textContent: fmt(value) });
  const input = el('input', { type: 'range', min, max, step, value });
  input.setAttribute('aria-label', label);
  input.addEventListener('input', () => { set(Number(input.value)); out.textContent = fmt(Number(input.value)); renderSaveBar(); });
  input.addEventListener('change', renderEditor);
  return el('label', { className: 'field' }, el('span', {}, `${label} `, out), input);
}

function renderRaw() {
  const raw = state.raw;
  $('raw-bytes').closest('details').hidden = raw.length === 0;
  const lines = [];
  for (let o = 0; o < raw.length; o += 16) {
    const hex = raw.slice(o, o + 16).map(b => b.toString(16).toUpperCase().padStart(2, '0')).join(' ');
    lines.push(`${o.toString(16).toUpperCase().padStart(2, '0')}  ${hex}`);
  }
  $('raw-bytes').textContent = lines.join('\n');
}

function renderSaveBar() {
  const changes = dirty() ? summarizeChanges(state.original, state.edited, buttonCount()) : [];
  $('savebar').hidden = changes.length === 0 || state.readOnly;
  $('save-summary').textContent = changes.length === 1 ? '1 unsaved change' : `${changes.length} unsaved changes`;
}

// ---------- actions ----------

async function save() {
  const changes = summarizeChanges(state.original, state.edited, buttonCount());
  if (!changes.length) return;
  const go = await ask({
    title: `Save to ${profileName(state.address)}?`,
    items: changes,
    note: 'The profile on the mouse is backed up first, so you can undo this from Backups.',
    ok: 'Save to mouse',
  });
  if (!go) return;
  busy(async () => {
    $('btn-save').disabled = true;
    $('btn-save').textContent = 'Saving…';
    try {
      const res = await api.saveProfile(state.address, state.original, state.edited);
      if (res.bytesChanged === 0) toast('Nothing changed on the mouse.');
      else toast(`Saved to ${profileName(state.address)}.${res.reloaded ? ' The mouse is using it now.' : ''} The old version is in Backups.`);
      await loadProfile(state.address);
      await loadBackups();
    } finally {
      $('btn-save').disabled = false;
      $('btn-save').textContent = 'Save to mouse';
    }
  });
}

async function restoreBackup(b) {
  const when = new Date(b.savedAt * 1000).toLocaleString();
  const go = await ask({
    title: `Put back ${profileName(b.address)} from ${when}?`,
    note: 'What’s on the mouse now is backed up first.',
    ok: 'Restore backup',
  });
  if (!go) return;
  busy(async () => {
    await api.restoreBackup(b.file);
    toast(`Restored ${profileName(b.address)}.`);
    if (b.address === state.address) await loadProfile(b.address);
    await loadBackups();
  });
}

// ---------- wiring ----------

$('btn-refresh').addEventListener('click', async () => {
  if (dirty() && !(await ask({ title: 'Throw away your changes?', note: 'Refresh reads the profile from the mouse again.', ok: 'Throw away changes' }))) return;
  busy(refresh);
});
$('btn-onboard').addEventListener('click', () => busy(async () => {
  await api.setOnboardMode(true);
  toast('The mouse now uses its own memory.');
  await refresh();
}));
$('btn-add-stage').addEventListener('click', () => {
  const s = state.edited;
  const last = s.dpiStages[s.dpiStages.length - 1] ?? 800;
  s.dpiStages.push(snapDpi(last >= DPI.max ? last - 400 : last + 400));
  renderEditor();
});
$('default-stage').addEventListener('change', e => { state.edited.defaultStage = Number(e.target.value); renderEditor(); });
$('shift-stage').addEventListener('change', e => { state.edited.shiftStage = Number(e.target.value); renderEditor(); });
$('tab-normal').addEventListener('click', () => { state.layer = 'buttons'; renderBindings(); });
$('tab-gshift').addEventListener('click', () => { state.layer = 'gshiftButtons'; renderBindings(); });
$('btn-undo').addEventListener('click', () => { state.edited = clone(state.original); renderEditor(); });
$('btn-save').addEventListener('click', save);
window.addEventListener('beforeunload', e => { if (dirty()) e.preventDefault(); });

$('preview-notice').hidden = !isPreview;
busy(refresh);
