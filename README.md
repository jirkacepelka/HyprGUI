# HyprGUI

GUI for configuring [Hyprland](https://hyprland.org): Rust + GTK4/libadwaita,
edits `hyprland.conf` in place without losing comments or formatting, and is
fully themable so distros and shells can ship their own look.

## Status

| Part | State |
| --- | --- |
| `hyprconf` lossless config parser/editor | done, tested |
| `hyprgui-theme` theme engine | done, tested |
| Themes `default`, `caelestia` | first version |
| GTK app, Hyprland IPC, option schema | planned |

See [docs/THEMING.md](docs/THEMING.md) to make a theme.

## Development

```
cargo test
cargo clippy --all-targets -- -D warnings
```
