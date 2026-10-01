//! Tiny translation layer: the English text is the key.

use std::sync::OnceLock;

static LANG: OnceLock<&'static str> = OnceLock::new();

pub fn lang() -> &'static str {
    LANG.get_or_init(hyprgui_core::language)
}

/// Translates `en` into the UI language; unknown strings stay English.
pub fn t(en: &'static str) -> &'static str {
    if lang() == "cs" {
        CS.iter().find(|(k, _)| *k == en).map_or(en, |(_, v)| v)
    } else {
        en
    }
}

const CS: &[(&str, &str)] = &[
    ("Appearance", "Vzhled"),
    ("Animations", "Animace"),
    ("Input", "Vstup"),
    ("Monitors", "Monitory"),
    ("Keybinds", "Klávesové zkratky"),
    ("Window rules", "Pravidla oken"),
    ("Autostart", "Autostart"),
    ("Application", "Aplikace"),
    ("Apply", "Použít"),
    ("Revert", "Vrátit"),
    ("Save", "Uložit"),
    ("Cancel", "Zrušit"),
    ("Discard", "Zahodit"),
    ("Delete", "Smazat"),
    ("Add", "Přidat"),
    ("Edit", "Upravit"),
    ("Saved", "Uloženo"),
    ("Changes reverted", "Změny vráceny"),
    ("Unsaved changes", "Neuložené změny"),
    (
        "Save the changes to your Hyprland config before closing?",
        "Uložit změny do konfigurace Hyprlandu před zavřením?",
    ),
    ("Could not save", "Uložení se nezdařilo"),
    (
        "Not running under Hyprland: changes are saved to the config but cannot be previewed live.",
        "Neběží pod Hyprlandem: změny se uloží do konfigurace, ale nelze je ukázat živě.",
    ),
    ("Show defined in", "Definováno v"),
    ("Defined in", "Definováno v"),
    ("Theme", "Téma"),
    ("Color scheme", "Barevné schéma"),
    ("Follow system", "Podle systému"),
    ("Light", "Světlé"),
    ("Dark", "Tmavé"),
    (
        "Look of HyprGUI itself. Themes are folders with a theme.toml, see docs/THEMING.md.",
        "Vzhled samotného HyprGUI. Témata jsou složky s theme.toml, viz docs/THEMING.md.",
    ),
    ("Open themes folder", "Otevřít složku s tématy"),
    ("Search", "Hledat"),
    ("Add keybind", "Přidat zkratku"),
    ("Edit keybind", "Upravit zkratku"),
    ("Modifiers", "Modifikátory"),
    ("Key", "Klávesa"),
    ("Record", "Nahrát"),
    ("Press a key combination…", "Stiskni kombinaci kláves…"),
    ("Action", "Akce"),
    ("Argument", "Argument"),
    ("Type", "Typ"),
    ("Description", "Popis"),
    (
        "Conflicts with another keybind",
        "Koliduje s jinou zkratkou",
    ),
    (
        "Keybinds that fire on the same combination are marked.",
        "Zkratky na stejnou kombinaci jsou označené.",
    ),
    ("No keybinds found", "Žádné zkratky nenalezeny"),
    ("Add a rule", "Přidat pravidlo"),
    ("Rule", "Pravidlo"),
    ("Window class", "Třída okna"),
    ("Window title", "Titulek okna"),
    ("Pick from open windows", "Vybrat z otevřených oken"),
    ("Rules", "Pravidla"),
    (
        "Add a rule for a window class or title.",
        "Přidej pravidlo pro třídu nebo titulek okna.",
    ),
    (
        "Commands run once when Hyprland starts",
        "Příkazy spuštěné jednou při startu Hyprlandu",
    ),
    ("Run on start", "Spustit při startu"),
    ("Run on every reload", "Spustit při každém načtení"),
    ("Environment variables", "Proměnné prostředí"),
    ("New entry", "Nový záznam"),
    ("Curves", "Křivky"),
    ("Bezier curves", "Bézierovy křivky"),
    ("Animation rules", "Pravidla animací"),
    ("Workspaces", "Plochy"),
    ("Workspace rules", "Pravidla ploch"),
    ("Layout", "Rozložení"),
    (
        "Drag monitors to arrange them.",
        "Přetažením uspořádej monitory.",
    ),
    ("Selected monitor", "Vybraný monitor"),
    ("Enabled", "Zapnuto"),
    (
        "Resolution and refresh rate",
        "Rozlišení a obnovovací frekvence",
    ),
    ("Scale", "Měřítko"),
    ("Rotation", "Otočení"),
    ("Preview", "Vyzkoušet"),
    (
        "Keep these display settings?",
        "Ponechat toto nastavení displejů?",
    ),
    ("Reverting in {} s…", "Vracím za {} s…"),
    ("Keep", "Ponechat"),
    ("Preferred", "Preferované"),
    ("Normal", "Normální"),
    ("No monitors detected", "Nebyl nalezen žádný monitor"),
    (
        "Start HyprGUI inside Hyprland, or add monitor lines to your config.",
        "Spusť HyprGUI v Hyprlandu, nebo přidej řádky monitor do konfigurace.",
    ),
    (
        "Display settings applied for preview",
        "Nastavení displejů vyzkoušeno",
    ),
    (
        "Previewing is not available outside Hyprland. Changes were written to the config.",
        "Mimo Hyprland nejde vyzkoušet. Změny byly zapsány do konfigurace.",
    ),
    ("Could not apply: {}", "Nepodařilo se použít: {}"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_duplicate_keys() {
        let mut seen = std::collections::HashSet::new();
        for (k, _) in CS {
            assert!(seen.insert(*k), "duplicate translation key {k}");
        }
    }

    #[test]
    fn unknown_strings_pass_through() {
        assert_eq!(t("definitely not translated"), "definitely not translated");
    }
}
