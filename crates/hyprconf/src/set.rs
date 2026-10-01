//! A main config plus every file it `source`s, edited as one unit.

use std::fs;
use std::path::{Path, PathBuf};

use crate::{Config, Error};

/// An assignment together with where it lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    /// Index into [`ConfigSet::files`].
    pub file: usize,
    /// Position among assignments of the same path inside that file.
    pub nth: usize,
    pub path: String,
    pub value: String,
}

struct File {
    path: PathBuf,
    config: Config,
    original: String,
    existed: bool,
}

/// What a file contained before `save` replaced it.
#[derive(Debug, Clone)]
pub struct Saved {
    pub path: PathBuf,
    /// `None` if the file did not exist.
    pub previous: Option<String>,
}

pub struct ConfigSet {
    files: Vec<File>,
}

fn expand(src: &str, base_dir: &Path) -> PathBuf {
    let src = src.trim();
    if let Some(rest) = src.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    let p = PathBuf::from(src);
    if p.is_relative() {
        base_dir.join(p)
    } else {
        p
    }
}

impl ConfigSet {
    /// Loads `main` and, recursively, its `source =` files. A missing main
    /// file yields an empty config so a fresh install can still be configured.
    pub fn load(main: impl AsRef<Path>) -> Result<Self, Error> {
        let mut set = ConfigSet { files: Vec::new() };
        set.load_file(main.as_ref(), true)?;
        Ok(set)
    }

    fn load_file(&mut self, path: &Path, is_main: bool) -> Result<(), Error> {
        let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if self.files.iter().any(|f| f.path == canon || f.path == path) {
            return Ok(()); // include cycle or duplicate include
        }
        let mut existed = true;
        let text = match fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if is_main && e.kind() == std::io::ErrorKind::NotFound => {
                existed = false;
                String::new()
            }
            Err(e) if !is_main => {
                // A missing or unreadable include must not block the GUI.
                let _ = e;
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        };
        let config = Config::parse(&text)?;
        let sources = config.get_all("source");
        self.files.push(File {
            path: path.to_path_buf(),
            config,
            original: text,
            existed,
        });
        let dir = path.parent().unwrap_or(Path::new("."));
        for s in sources {
            // Included files that fail to parse are skipped, not fatal.
            let _ = self.load_file(&expand(&s, dir), false);
        }
        Ok(())
    }

    pub fn files(&self) -> Vec<&Path> {
        self.files.iter().map(|f| f.path.as_path()).collect()
    }

    pub fn main_path(&self) -> &Path {
        &self.files[0].path
    }

    /// All assignments in Hyprland evaluation order: an included file's
    /// entries appear where its `source` line is.
    pub fn entries(&self) -> Vec<Located> {
        let mut out = Vec::new();
        let mut visited = Vec::new();
        self.walk(0, &mut visited, &mut out);
        out
    }

    fn walk(&self, idx: usize, visited: &mut Vec<usize>, out: &mut Vec<Located>) {
        if visited.contains(&idx) {
            return;
        }
        visited.push(idx);
        let f = &self.files[idx];
        let dir = f.path.parent().unwrap_or(Path::new("."));
        let mut counts: std::collections::HashMap<String, usize> = Default::default();
        for e in f.config.entries() {
            let n = counts.entry(e.path.clone()).or_insert(0);
            let nth = *n;
            *n += 1;
            if e.path == "source" {
                let target = expand(&e.value, dir);
                if let Some(j) = self.files.iter().position(|x| x.path == target) {
                    self.walk(j, visited, out);
                }
                continue;
            }
            out.push(Located {
                file: idx,
                nth,
                path: e.path,
                value: e.value,
            });
        }
    }

    pub fn get(&self, path: &str) -> Option<Located> {
        self.entries().into_iter().rev().find(|e| e.path == path)
    }

    pub fn get_all(&self, path: &str) -> Vec<Located> {
        self.entries()
            .into_iter()
            .filter(|e| e.path == path)
            .collect()
    }

    /// `$name = value` definitions, later ones winning.
    pub fn variables(&self) -> Vec<(String, String)> {
        let mut vars: Vec<(String, String)> = Vec::new();
        for e in self.entries() {
            if e.path.starts_with('$') {
                vars.retain(|(k, _)| *k != e.path);
                vars.push((e.path, e.value));
            }
        }
        vars
    }

    /// Sets a single-valued option in the file that currently defines it,
    /// otherwise in the main file. Returns the file index used.
    pub fn set(&mut self, path: &str, value: &str) -> usize {
        let target = self.get(path).map_or(0, |l| l.file);
        self.files[target].config.set(path, value);
        target
    }

    pub fn replace(&mut self, loc: &Located, value: &str) -> bool {
        self.files[loc.file]
            .config
            .replace_nth(&loc.path, loc.nth, value)
    }

    pub fn remove(&mut self, loc: &Located) -> bool {
        self.files[loc.file].config.remove_nth(&loc.path, loc.nth)
    }

    /// New list items go to the main file.
    pub fn push(&mut self, path: &str, value: &str) {
        self.files[0].config.push(path, value);
    }

    pub fn is_dirty(&self) -> bool {
        self.files
            .iter()
            .any(|f| f.config.to_string() != f.original)
    }

    pub fn dirty_files(&self) -> Vec<&Path> {
        self.files
            .iter()
            .filter(|f| f.config.to_string() != f.original)
            .map(|f| f.path.as_path())
            .collect()
    }

    /// Writes every changed file (with `.bak`). On an I/O error the files
    /// already written are restored. Returns what was replaced so the caller
    /// can [`rollback`](Self::rollback) if the result fails validation.
    pub fn save(&mut self) -> Result<Vec<Saved>, Error> {
        let mut saved: Vec<Saved> = Vec::new();
        for i in 0..self.files.len() {
            if self.files[i].config.to_string() == self.files[i].original {
                continue;
            }
            if let Some(dir) = self.files[i].path.parent() {
                fs::create_dir_all(dir)?;
            }
            let previous = self.files[i]
                .existed
                .then(|| self.files[i].original.clone());
            if let Err(e) = self.files[i].config.save(&self.files[i].path) {
                self.restore(&saved);
                return Err(e);
            }
            saved.push(Saved {
                path: self.files[i].path.clone(),
                previous,
            });
        }
        for f in &mut self.files {
            if saved.iter().any(|s| s.path == f.path) {
                f.original = f.config.to_string();
                f.existed = true;
            }
        }
        Ok(saved)
    }

    fn restore(&self, saved: &[Saved]) {
        for s in saved {
            match &s.previous {
                Some(text) => {
                    let _ = fs::write(&s.path, text);
                }
                None => {
                    let _ = fs::remove_file(&s.path);
                }
            }
        }
    }

    /// Puts the files back as they were before the given `save`. The in-memory
    /// edits stay, so the set is dirty again and the user can fix and retry.
    pub fn rollback(&mut self, saved: &[Saved]) {
        self.restore(saved);
        for f in &mut self.files {
            if let Some(s) = saved.iter().find(|s| s.path == f.path) {
                f.original = s.previous.clone().unwrap_or_default();
                f.existed = s.previous.is_some();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(n: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("hyprconf-set-{n}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn follows_sources_in_order_and_last_wins() {
        let d = tmpdir("order");
        fs::write(
            d.join("hyprland.conf"),
            "general {\n gaps_in = 1\n}\nsource = colors.conf\ngeneral {\n gaps_out = 9\n}\n",
        )
        .unwrap();
        fs::write(d.join("colors.conf"), "general {\n gaps_in = 7\n}\n").unwrap();
        let s = ConfigSet::load(d.join("hyprland.conf")).unwrap();
        assert_eq!(s.files().len(), 2);
        assert_eq!(s.get("general:gaps_in").unwrap().value, "7");
        assert_eq!(s.get("general:gaps_in").unwrap().file, 1);
        assert_eq!(s.get("general:gaps_out").unwrap().file, 0);
        fs::remove_dir_all(d).ok();
    }

    #[test]
    fn set_writes_where_defined_and_new_goes_to_main() {
        let d = tmpdir("set");
        fs::write(d.join("hyprland.conf"), "source = inc.conf\n").unwrap();
        fs::write(d.join("inc.conf"), "general {\n gaps_in = 7\n}\n").unwrap();
        let mut s = ConfigSet::load(d.join("hyprland.conf")).unwrap();
        assert_eq!(s.set("general:gaps_in", "3"), 1);
        assert_eq!(s.set("general:border_size", "2"), 0);
        assert_eq!(s.dirty_files().len(), 2);
        s.save().unwrap();
        assert!(fs::read_to_string(d.join("inc.conf"))
            .unwrap()
            .contains("gaps_in = 3"));
        assert!(fs::read_to_string(d.join("hyprland.conf"))
            .unwrap()
            .contains("border_size = 2"));
        assert!(!s.is_dirty());
        fs::remove_dir_all(d).ok();
    }

    #[test]
    fn include_cycle_and_missing_include_are_harmless() {
        let d = tmpdir("cycle");
        fs::write(
            d.join("hyprland.conf"),
            "source = a.conf\nsource = nope.conf\n",
        )
        .unwrap();
        fs::write(d.join("a.conf"), "source = hyprland.conf\nx = 1\n").unwrap();
        let s = ConfigSet::load(d.join("hyprland.conf")).unwrap();
        assert_eq!(s.get("x").unwrap().value, "1");
        fs::remove_dir_all(d).ok();
    }

    #[test]
    fn missing_main_is_empty_and_created_on_save() {
        let d = tmpdir("fresh");
        let p = d.join("hypr/hyprland.conf");
        let mut s = ConfigSet::load(&p).unwrap();
        s.set("general:gaps_in", "4");
        s.save().unwrap();
        assert!(fs::read_to_string(&p).unwrap().contains("gaps_in = 4"));
        fs::remove_dir_all(d).ok();
    }

    #[test]
    fn list_items_are_editable_across_files() {
        let d = tmpdir("list");
        fs::write(
            d.join("hyprland.conf"),
            "bind = A, a, exec, x\nsource = b.conf\n",
        )
        .unwrap();
        fs::write(d.join("b.conf"), "bind = B, b, exec, y\n").unwrap();
        let mut s = ConfigSet::load(d.join("hyprland.conf")).unwrap();
        let items = s.get_all("bind");
        assert_eq!(items.len(), 2);
        assert!(s.replace(&items[1], "B, b, exec, z"));
        assert!(s.remove(&items[0]));
        s.push("bind", "C, c, exit");
        let vals: Vec<_> = s.get_all("bind").into_iter().map(|l| l.value).collect();
        // C is appended after the `source` line, so it evaluates after B.
        assert_eq!(vals, vec!["B, b, exec, z", "C, c, exit"]);
        fs::remove_dir_all(d).ok();
    }

    #[test]
    fn rollback_restores_files_and_keeps_edits() {
        let d = tmpdir("rollback");
        let p = d.join("hyprland.conf");
        fs::write(&p, "a = 1\n").unwrap();
        let mut s = ConfigSet::load(&p).unwrap();
        s.set("a", "2");
        let saved = s.save().unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "a = 2\n");
        s.rollback(&saved);
        assert_eq!(fs::read_to_string(&p).unwrap(), "a = 1\n");
        assert!(s.is_dirty());
        assert_eq!(s.get("a").unwrap().value, "2");
        fs::remove_dir_all(d).ok();
    }

    #[test]
    fn rollback_removes_a_file_that_did_not_exist() {
        let d = tmpdir("rollback-new");
        let p = d.join("new.conf");
        let mut s = ConfigSet::load(&p).unwrap();
        s.set("a", "1");
        let saved = s.save().unwrap();
        s.rollback(&saved);
        assert!(!p.exists());
        fs::remove_dir_all(d).ok();
    }

    #[test]
    fn variables_resolve() {
        let d = tmpdir("vars");
        fs::write(
            d.join("hyprland.conf"),
            "$mod = SUPER\n$term = kitty\n$mod = ALT\n",
        )
        .unwrap();
        let s = ConfigSet::load(d.join("hyprland.conf")).unwrap();
        let v = s.variables();
        assert!(v.contains(&("$mod".into(), "ALT".into())));
        assert_eq!(v.len(), 2);
        fs::remove_dir_all(d).ok();
    }
}
