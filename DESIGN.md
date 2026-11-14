# G502 Hub design

**Character:** exact, quiet, hands-on. **World:** a hardware spec sheet next to the matte-black mouse.

## Tokens (see `src/app.css`)
- Neutral greys (hue 260, near-zero chroma) for background, surface, border and text. Light and dark follow the OS.
- **One accent:** signal orange, `oklch(0.56 0.16 48)` light / `oklch(0.74 0.15 55)` dark. Only for the primary action, selected segments, the current profile and the starting-stage underline.
- Warn notices: a pale amber background with dark amber text. Errors: a red toast.
- Type: Segoe UI Variable (native Windows), tabular numbers everywhere numbers appear. Cascadia Mono only for the raw-bytes panel.
- Shape: radius 6 (controls) and 10 (cards, dialogs). Borders only, no shadows. Spacing on 4/8.

## Layout
Left rail (260px): mouse status, profiles on the mouse, backups. Main column (max 980px): Sensitivity, Buttons, Lighting, raw bytes. A save bar pinned to the bottom appears only when there are unsaved changes. Below 760px the rail stacks on top.

**Signature:** DPI stages as big numbers in a row of cards; the starting stage is underlined in the accent.

## Rules
- Labels say what happens: "Save to mouse", "Undo changes", "Use the mouse's own memory", "Restore".
- Nothing reaches the mouse without the confirm dialog, which lists every change in plain words.
- Unknown values are shown as "Unknown (…)" and kept as is; never silently replaced.

## Never
- More than one accent, gradients, shadows on cards, emoji icons.
- Auto-saving or writing on every slider move.
- Guessing physical names for slots 6–11 or the lighting zones until confirmed on the real mouse.
