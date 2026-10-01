# Theming HyprGUI

A theme is a directory. Distros and shells ship one by installing it into
`/usr/share/hyprgui/themes/<id>/`; users put theirs in
`~/.config/hyprgui/themes/<id>/`. Extra directories can be added with
`HYPRGUI_THEME_PATH` (colon separated). Earlier paths win when ids collide.

```
<id>/
  theme.toml   required: metadata + tokens
  style.css    optional GTK CSS, appended after the generated CSS
  assets/      optional
```

## theme.toml

```toml
[theme]
id = "my-shell"          # [A-Za-z0-9_-]
name = "My Shell"
author = "You"
base = "dark"            # variant used without a system preference

[fonts]                  # all optional
ui = "Inter"
mono = "JetBrains Mono"
icons = "Material Symbols Rounded"

[shape]                  # px, all optional
radius_small = 8
radius_medium = 16
radius_large = 28

[colors.dark]            # Material 3 token names, #rrggbb
primary = "#d0bcff"
# ...

[colors.light]
# ...

[dynamic]                # optional: follow a shell's wallpaper-driven scheme
source = "~/.local/state/my-shell/scheme.json"
```

Anything you leave out falls back to the built-in default, so a theme can be
three lines long.

### Colour tokens

Required (a warning is raised if a defined variant lacks one):
`primary on_primary primary_container on_primary_container secondary
on_secondary tertiary error on_error background on_background surface
on_surface on_surface_variant surface_container_low surface_container
surface_container_high outline outline_variant`

Each token becomes `@m3_<token>` in CSS and is mapped onto libadwaita names
(`accent_bg_color`, `window_bg_color`, `card_bg_color`, `headerbar_bg_color`,
`sidebar_bg_color`, `popover_bg_color`, …), so stock widgets pick it up
without extra CSS.

### Dynamic source

A flat JSON file of `"token": "#hex"` pairs (leading `#` optional). Non-string
values are ignored. Values override the static colours of the active variant;
a missing or unreadable file is ignored. HyprGUI reloads the theme when the
file changes.

## style.css

Plain GTK4 CSS. The window has the class `hyprgui`; the hero banner uses
`.hero`; icon labels use `.symbol`. See `themes/caelestia/style.css`.

## Checking a theme

```
cargo run -p hyprgui -- theme check path/to/theme    # planned CLI
```
Checks: valid ids and colours, missing tokens, and WCAG contrast (4.5:1) of
text/background pairs. The `hyprgui-theme` crate's `Theme::validate` does the
same today.

## Shipped themes

- `default`: neutral blue, libadwaita look.
- `caelestia`: soft purple Material 3 palette, pill navigation, large radii,
  Google Sans Flex. Palette is the M3 baseline and still to be matched to the
  Caelestia shell's own scheme.
