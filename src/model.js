// Pure data helpers: action catalogue, key names, labels and change summaries.
// No DOM here, so it can be unit-tested with `deno test`.

export const MOD = { ctrl: 1, shift: 2, alt: 4, win: 8 };

// Special mouse functions (`0x90 <id>` bindings). Names follow libratbag's
// HID++ 2.0 table; not yet confirmed on a G502 HERO, see README.
export const SPECIALS = {
  0x01: 'Scroll left',
  0x02: 'Scroll right',
  0x03: 'DPI up',
  0x04: 'DPI down',
  0x05: 'Cycle DPI',
  0x06: 'Back to starting DPI',
  0x07: 'Sniper (hold for DPI shift)',
  0x08: 'Next profile',
  0x09: 'Previous profile',
  0x0a: 'Cycle profiles',
  0x0b: 'G-Shift (hold)',
};

export const MOUSE_BUTTONS = { 1: 'Left click', 2: 'Right click', 3: 'Wheel click', 4: 'Back', 5: 'Forward' };

export const MEDIA = {
  0xcd: 'Play / pause',
  0xb5: 'Next track',
  0xb6: 'Previous track',
  0xe9: 'Volume up',
  0xea: 'Volume down',
  0xe2: 'Mute',
};

/** Names of the 11 physical button slots, in profile order. */
export function slotName(i) {
  return ['Left button', 'Right button', 'Wheel click', 'Back button', 'Forward button'][i] ?? `Button ${i + 1}`;
}

// KeyboardEvent.code -> HID keyboard usage id.
export const KEY_USAGE = (() => {
  const m = {};
  for (let i = 0; i < 26; i++) m[`Key${String.fromCharCode(65 + i)}`] = 0x04 + i;
  for (let i = 1; i <= 9; i++) m[`Digit${i}`] = 0x1d + i;
  m.Digit0 = 0x27;
  Object.assign(m, {
    Enter: 0x28, Escape: 0x29, Backspace: 0x2a, Tab: 0x2b, Space: 0x2c, Minus: 0x2d, Equal: 0x2e,
    BracketLeft: 0x2f, BracketRight: 0x30, Backslash: 0x31, Semicolon: 0x33, Quote: 0x34,
    Backquote: 0x35, Comma: 0x36, Period: 0x37, Slash: 0x38, CapsLock: 0x39,
    PrintScreen: 0x46, ScrollLock: 0x47, Pause: 0x48, Insert: 0x49, Home: 0x4a, PageUp: 0x4b,
    Delete: 0x4c, End: 0x4d, PageDown: 0x4e, ArrowRight: 0x4f, ArrowLeft: 0x50, ArrowDown: 0x51,
    ArrowUp: 0x52, NumLock: 0x53, NumpadDivide: 0x54, NumpadMultiply: 0x55, NumpadSubtract: 0x56,
    NumpadAdd: 0x57, NumpadEnter: 0x58, Numpad0: 0x62, NumpadDecimal: 0x63, ContextMenu: 0x65,
  });
  for (let i = 1; i <= 9; i++) m[`Numpad${i}`] = 0x58 + i;
  for (let i = 1; i <= 12; i++) m[`F${i}`] = 0x39 + i;
  for (let i = 13; i <= 24; i++) m[`F${i}`] = 0x68 + (i - 13);
  return m;
})();

const PRETTY = {
  BracketLeft: '[', BracketRight: ']', Backslash: '\\', Semicolon: ';', Quote: "'", Backquote: '`',
  Comma: ',', Period: '.', Slash: '/', Minus: '-', Equal: '=', ArrowRight: '→', ArrowLeft: '←',
  ArrowDown: '↓', ArrowUp: '↑', Escape: 'Esc', Delete: 'Del', PageUp: 'Page Up', PageDown: 'Page Down',
  PrintScreen: 'Print Screen', ScrollLock: 'Scroll Lock', CapsLock: 'Caps Lock', NumLock: 'Num Lock',
  ContextMenu: 'Menu',
};

export const USAGE_NAME = Object.fromEntries(
  Object.entries(KEY_USAGE).map(([code, usage]) => [
    usage,
    PRETTY[code] ?? code.replace(/^Key|^Digit/, '').replace(/^Numpad/, 'Num '),
  ]),
);

export function modifierNames(mods) {
  const names = [];
  // Left and right variants share a name.
  const m = (mods & 0x0f) | (mods >> 4);
  if (m & MOD.ctrl) names.push('Ctrl');
  if (m & MOD.shift) names.push('Shift');
  if (m & MOD.alt) names.push('Alt');
  if (m & MOD.win) names.push('Win');
  return names;
}

const hex = n => n.toString(16).toUpperCase().padStart(2, '0');

export function bindingLabel(b) {
  switch (b.kind) {
    case 'mouse': return MOUSE_BUTTONS[b.button] ?? `Mouse button ${b.button}`;
    case 'key': return [...modifierNames(b.modifiers), USAGE_NAME[b.key] ?? `Key ${hex(b.key)}`].join(' + ');
    case 'media': return MEDIA[b.usage] ?? `Media key ${b.usage.toString(16).toUpperCase()}`;
    case 'special': return SPECIALS[b.action] ?? `Unknown function (${hex(b.action)})`;
    case 'disabled': return 'Does nothing';
    case 'raw': return `Unknown (${b.bytes.map(hex).join(' ')})`;
    default: return 'Unknown';
  }
}

export function lightingLabel(l) {
  switch (l.kind) {
    case 'off': return 'Off';
    case 'fixed': return `Colour ${rgbToHex(l.color)}`;
    case 'breathing': return `Breathing ${rgbToHex(l.color)}, ${l.periodMs / 1000}s, ${l.brightness}%`;
    case 'cycle': return `Colour cycle, ${l.periodMs / 1000}s, ${l.brightness}%`;
    default: return 'Unknown effect';
  }
}

export function rgbToHex([r, g, b]) {
  return `#${[r, g, b].map(v => v.toString(16).padStart(2, '0')).join('')}`.toUpperCase();
}

export function hexToRgb(h) {
  const v = parseInt(h.replace('#', ''), 16);
  return [(v >> 16) & 255, (v >> 8) & 255, v & 255];
}

export function equal(a, b) {
  return JSON.stringify(a) === JSON.stringify(b);
}

export function clone(v) {
  return JSON.parse(JSON.stringify(v));
}

export const RATES = [125, 250, 500, 1000];
export const DPI = { min: 100, max: 25600, step: 50 };

export function snapDpi(v) {
  const n = Math.round(Number(v) / DPI.step) * DPI.step;
  if (!Number.isFinite(n)) return DPI.min;
  return Math.min(DPI.max, Math.max(DPI.min, n));
}

/** Plain-English list of what differs between two Settings objects. */
export function summarizeChanges(before, after, buttonCount = 11) {
  const out = [];
  if (before.reportRateHz !== after.reportRateHz) {
    out.push(`Polling rate: ${before.reportRateHz ?? '?'} → ${after.reportRateHz} Hz`);
  }
  if (!equal(before.dpiStages, after.dpiStages)) {
    out.push(`DPI stages: ${before.dpiStages.join(', ')} → ${after.dpiStages.join(', ')}`);
  }
  if (before.defaultStage !== after.defaultStage || !equal(before.dpiStages, after.dpiStages)) {
    const was = before.dpiStages[before.defaultStage];
    const now = after.dpiStages[after.defaultStage];
    if (was !== now) out.push(`Starting DPI: ${was} → ${now}`);
  }
  if (before.shiftStage !== after.shiftStage || !equal(before.dpiStages, after.dpiStages)) {
    const was = before.dpiStages[before.shiftStage];
    const now = after.dpiStages[after.shiftStage];
    if (was !== now) out.push(`Sniper DPI: ${was} → ${now}`);
  }
  for (const [list, layer] of [['buttons', ''], ['gshiftButtons', ' (G-Shift)']]) {
    for (let i = 0; i < buttonCount; i++) {
      if (!equal(before[list][i], after[list][i])) {
        out.push(`${slotName(i)}${layer}: ${bindingLabel(before[list][i])} → ${bindingLabel(after[list][i])}`);
      }
    }
  }
  after.lighting.forEach((l, z) => {
    if (!equal(before.lighting[z], l)) {
      out.push(`Lighting zone ${z + 1}: ${lightingLabel(before.lighting[z])} → ${lightingLabel(l)}`);
    }
  });
  return out;
}
