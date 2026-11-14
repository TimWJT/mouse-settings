import { test } from 'node:test';
import { deepStrictEqual as assertEquals } from 'node:assert/strict';
import sample from '../src/sample-profile.json' with { type: 'json' };
import { bindingLabel, summarizeChanges, snapDpi, KEY_USAGE, clone, rgbToHex, hexToRgb } from '../src/model.js';

test('sample bindings get readable labels', () => {
  assertEquals(bindingLabel(sample.buttons[0]), 'Left click');
  assertEquals(bindingLabel(sample.buttons[5]), ']');
  assertEquals(bindingLabel(sample.buttons[6]), 'Alt + Tab');
  assertEquals(bindingLabel(sample.buttons[8]), 'Cycle profiles');
  assertEquals(bindingLabel(sample.buttons[11]), 'Does nothing');
  assertEquals(bindingLabel({ kind: 'raw', bytes: [0, 1, 0, 64] }), 'Unknown (00 01 00 40)');
});

test('key map matches HID usage ids', () => {
  assertEquals(KEY_USAGE.KeyA, 0x04);
  assertEquals(KEY_USAGE.KeyZ, 0x1d);
  assertEquals(KEY_USAGE.Digit1, 0x1e);
  assertEquals(KEY_USAGE.Digit0, 0x27);
  assertEquals(KEY_USAGE.Tab, 0x2b);
  assertEquals(KEY_USAGE.F1, 0x3a);
  assertEquals(KEY_USAGE.F12, 0x45);
  assertEquals(KEY_USAGE.F13, 0x68);
  assertEquals(KEY_USAGE.Numpad1, 0x59);
  assertEquals(KEY_USAGE.Numpad9, 0x61);
});

test('change summary lists only what changed', () => {
  assertEquals(summarizeChanges(sample, clone(sample)), []);
  const after = clone(sample);
  after.reportRateHz = 1000;
  after.dpiStages[2] = 1600;
  after.buttons[3] = { kind: 'special', action: 3 };
  after.lighting[0] = { kind: 'off' };
  assertEquals(summarizeChanges(sample, after), [
    'Polling rate: 500 → 1000 Hz',
    'DPI stages: 300, 550, 900 → 300, 550, 1600',
    'Starting DPI: 900 → 1600',
    'Sniper DPI: 900 → 1600',
    'Back button: Back → DPI up',
    'Lighting zone 1: Colour #FF0004 → Off',
  ]);
});

test('DPI snaps to the sensor range in 50s', () => {
  assertEquals(snapDpi(1624), 1600);
  assertEquals(snapDpi(1625), 1650);
  assertEquals(snapDpi(5), 100);
  assertEquals(snapDpi(99999), 25600);
  assertEquals(snapDpi('abc'), 100);
});

test('colour hex round trip', () => {
  assertEquals(rgbToHex([255, 0, 4]), '#FF0004');
  assertEquals(hexToRgb('#ff0004'), [255, 0, 4]);
});
