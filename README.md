# Mouse Settings

An offline settings app for gaming mice. It currently supports only the **Logitech G502 HERO** (wired, USB `046D:C08B`). It edits the profiles stored on the mouse itself: DPI stages, polling rate, buttons (normal and G-Shift) and the two lighting zones. Once saved, the settings live in the mouse and work on any PC without software.

No account, no network access, no background service.

Unofficial: not made by, affiliated with or endorsed by Logitech. Source: https://github.com/TimWJT/mouse-settings. Previously called G502 Hub.

## How it keeps your mouse safe

1. **Edits start from what's on the mouse.** The app reads the profile, you change things, and on save it re-reads the profile and changes only the fields you touched. If the profile on the mouse changed since you loaded it, nothing is written and you're asked to Refresh. Anything it doesn't understand (macros, unknown lighting effects, reserved bytes) is kept byte for byte.
2. **Checks before writing.** It refuses unless: the profile's checksum is valid, the mouse uses 256-byte memory sectors, the profile list's checksum is valid, and the profile slot is switched on in that list. It never writes the profile list (sector 0) or the factory profiles.
3. **Backup first.** The old profile is saved to the app's data folder before every write. If the backup fails, nothing is written. Restore any backup from the sidebar.
4. **Read-back.** After writing, it reads the profile back and compares every byte. If anything fails once writing has started, the error names the backup to restore.

## Running it

```bash
npm install
```

```bash
npm run app:dev
```

`npm run app:build` makes the installer. Close Logitech G Hub first; it can hold the mouse's settings interface.

Windows is the tested platform. Linux and macOS builds use the same code through `hidapi` but haven't been tried on a real mouse. On Linux, building needs `libudev-dev`, and the app needs access to the mouse's hidraw device, e.g. `/etc/udev/rules.d/70-mouse-settings.rules`:

```
KERNEL=="hidraw*", ATTRS{idVendor}=="046d", ATTRS{idProduct}=="c08b", TAG+="uaccess"
```

To look at the page without the mouse (uses a built-in sample profile):

```bash
npm run preview
```

## Tests

```bash
npm test
```

```bash
npm run test:rust
```

The Rust tests run every operation against a simulated mouse that parses the real HID++ byte layout and rejects wrong layouts, so a framing mistake fails a test rather than touching hardware.

## Where the protocol details come from

Clean-room: written from public sources only, with no Logitech code and nothing decompiled.

| What | Source | Confidence |
| --- | --- | --- |
| HID++ 2.0 framing, IRoot, feature lookup | Logitech's public HID++ 2.0 docs, Solaar, libratbag | High |
| `0x8100` onboard profiles: read/write/mode/current profile | libratbag (MIT), Solaar behaviour | High |
| Profile layout: DPI, report rate, button and name offsets, CRC | libratbag `hidpp20_internal_profile`, checked against a real G502 HERO dump | High |
| Button types: mouse, key, media, disabled | libratbag; mouse and key confirmed in the real dump | High |
| Special function ids (DPI up/down, sniper, profile cycle…) | libratbag's table | **Medium: confirm on the mouse** |
| Lighting: off and fixed colour | libratbag; fixed confirmed in the real dump | High |
| Lighting: breathing and colour cycle byte layout | libratbag | **Medium: confirm on the mouse** |
| Which lighting zone is the logo and which the DPI lights | not known | Labelled "zone 1/2" |
| Which slot is which physical G-button (slots 6–11) | not known | Labelled "Button 6…11" |

First check on real hardware: press Refresh and compare what the app shows with what the buttons actually do now. If they match, the medium-confidence rows are right.

## Layout

```
src-tauri/src/
  main.rs        Tauri commands (async; the UI thread never touches USB)
  worker.rs      one thread owns the USB handle and runs jobs in order
  hidpp/         HID++ 2.0 framing, feature-index cache, hidapi transport
  device.rs      onboard-profile operations and the write safety checks
  profile.rs     lossless 256-byte profile codec
  backup.rs      backup files
  mock.rs        simulated mouse for tests
src/
  main.js        page: render + collect edits
  model.js       labels, key codes, change summaries (unit-tested)
  api.js         backend calls, or a sample mouse in a plain browser
```

The previous version (v1) is archived in `../_archive/`.
