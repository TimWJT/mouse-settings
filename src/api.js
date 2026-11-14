// Talks to the Rust backend inside the desktop app, or to a sample mouse in a plain browser
// (so the page can be previewed and tested without hardware).

import sample from './sample-profile.json';
import { clone, equal } from './model.js';

const tauri = typeof window !== 'undefined' && window.__TAURI_INTERNALS__;

export const isPreview = !tauri;

function invoke(cmd, args) {
  return tauri.invoke(cmd, args);
}

const live = {
  mouseInfo: () => invoke('mouse_info'),
  readProfile: address => invoke('read_profile', { address }),
  saveProfile: (address, base, settings) => invoke('save_profile', { address, base, settings }),
  setOnboardMode: onboard => invoke('set_onboard_mode', { onboard }),
  listBackups: () => invoke('list_backups'),
  restoreBackup: file => invoke('restore_backup', { file }),
};

// ---- Preview: an in-memory mouse seeded with a real G502 HERO profile ----

const store = { 1: clone(sample), 2: clone(sample), 257: clone(sample) };
let mode = 'onboard';
const backups = [];
const wait = ms => new Promise(r => setTimeout(r, ms));

const preview = {
  async mouseInfo() {
    await wait(150);
    return {
      name: 'G502 HERO (sample)',
      firmware: 'U1 27.03.B0016',
      protocol: '4.2',
      mode,
      description: { profileCount: 5, buttonCount: 11, sectorCount: 16, sectorSize: 256 },
      slots: [
        { address: 1, enabled: true, writable: true },
        { address: 2, enabled: false, writable: true },
        { address: 257, enabled: true, writable: false },
      ],
      currentProfile: 1,
    };
  },
  async readProfile(address) {
    await wait(120);
    return { address, settings: clone(store[address]), crcOk: true, raw: [] };
  },
  async saveProfile(address, base, settings) {
    await wait(500);
    if (!equal(store[address], base)) throw new Error('Nothing was written: the profile on the mouse changed since it was loaded.');
    if (equal(store[address], settings)) return { backupFile: null, bytesChanged: 0, reloaded: false };
    const file = `profile-${address}-${Math.floor(Date.now() / 1000)}.bin`;
    backups.unshift({ file, address, savedAt: Math.floor(Date.now() / 1000), data: clone(store[address]) });
    store[address] = clone(settings);
    return { backupFile: file, bytesChanged: 4, reloaded: true };
  },
  async setOnboardMode(onboard) {
    await wait(100);
    mode = onboard ? 'onboard' : 'host';
  },
  async listBackups() {
    return backups.map(({ data, ...rest }) => rest);
  },
  async restoreBackup(file) {
    const b = backups.find(x => x.file === file);
    return preview.saveProfile(b.address, store[b.address], b.data);
  },
};

export const api = tauri ? live : preview;
