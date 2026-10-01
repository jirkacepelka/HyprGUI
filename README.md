# HyprGUI

Settings GUI for [Hyprland](https://hyprland.org), built with Rust and
GTK4/libadwaita. It edits your `hyprland.conf` in place (comments, ordering and
`source =` includes are preserved), previews changes live through Hyprland's
IPC, and is fully themable so distros and shells can ship their own look.

## Features

- **Appearance**: gaps, borders (gradients), rounding, opacity, blur, shadows
- **Animations**: bezier curves and animation rules
- **Input**: keyboard layouts and repeat, mouse, touchpad, gestures
- **Monitors**: drag-and-drop layout with edge snapping, resolution / refresh
  rate / scale / rotation, 15 s "keep these settings?" countdown that reverts
  automatically, workspace rules
- **Keybinds**: searchable list, key recorder, conflict detection
- **Window rules** (with a picker for open windows) and **Autostart** / env
- **Themes**: switch at runtime, hot-reload while you edit one. See
  [docs/THEMING.md](docs/THEMING.md). Ships `default` and `caelestia`.
- English and Czech UI (follows `$LANG`)

Safety: every save writes `<file>.bak`, then checks the result with
`Hyprland --verify-config` (when available) and restores the old files if the
check fails. Unsaved edits can be reverted, which also restores the live state.

## Install

### Arch / CachyOS

```
git clone https://github.com/jirkacepelka/HyprGUI
cd HyprGUI/packaging
makepkg -si
```

### From source

Needs Rust, GTK 4.12+ and libadwaita 1.5+ (`gtk4`, `libadwaita` on Arch,
`libgtk-4-dev libadwaita-1-dev` on Debian/Ubuntu).

```
cargo build --release -p hyprgui
HYPRGUI_THEME_PATH=themes ./target/release/hyprgui
```

Installed themes live in `/usr/share/hyprgui/themes`; your own go in
`~/.config/hyprgui/themes`.

## Use

```
hyprgui [--config FILE] [--theme ID] [--page ID] [--no-ipc]
hyprgui theme list
hyprgui theme check <DIR|ID>
```

Run it inside a Hyprland session for live preview. Elsewhere it still edits
the config, without preview. Back up `~/.config/hypr` before the first run.

## Known limits

- Hyprland 0.51 replaced `gestures:workspace_swipe*` with `gesture =` rules.
  The Input page shows the old options; add new-style gestures as lines in
  the config for now.
- The window rule dialog writes legacy `windowrulev2` lines. Newer Hyprland
  syntax can be edited as plain text in the same list.
- The `caelestia` palette is Material 3 baseline purple, not yet matched to
  the Caelestia shell's own scheme.

## Development

```
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Crates: `hyprconf` (lossless config editor), `hypripc` (Hyprland socket),
`hyprschema` (option schema), `hyprgui-core` (session and models),
`hyprgui-theme` (theme engine), `hyprgui` (the app).
