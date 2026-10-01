//! An editing session: config files + live Hyprland connection.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use hyprconf::{ConfigSet, Located};
use hypripc::Client;
use hyprschema::{Opt, Schema};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Config(#[from] hyprconf::Error),
    #[error("`{value}` is not a valid value for {key}")]
    Invalid { key: String, value: String },
    #[error("Hyprland rejected the saved config, changes were rolled back:\n{0}")]
    Verify(String),
}

/// Outcome of trying to preview a change live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveStatus {
    Applied,
    /// Not running under Hyprland.
    NoIpc,
    /// The option cannot be changed without a reload.
    NotLive,
    Failed(String),
}

#[derive(Debug, Clone, Default)]
pub struct SaveReport {
    pub written: Vec<PathBuf>,
    /// Set when the reload request failed (config was still saved).
    pub reload_error: Option<String>,
}

pub struct Session {
    pub schema: Schema,
    cfg: ConfigSet,
    ipc: Option<Client>,
    /// Option keys previewed live since the last save/revert.
    touched: BTreeSet<String>,
    verify: bool,
}

impl Session {
    /// `$XDG_CONFIG_HOME/hypr/hyprland.conf` (or `~/.config/...`).
    pub fn default_config_path() -> PathBuf {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("hypr/hyprland.conf")
    }

    pub fn open(path: impl AsRef<Path>, ipc: Option<Client>) -> Result<Session, Error> {
        Ok(Session {
            schema: Schema::builtin(),
            cfg: ConfigSet::load(path)?,
            ipc,
            touched: BTreeSet::new(),
            verify: true,
        })
    }

    /// Disable `Hyprland --verify-config` after saving (tests, unusual setups).
    pub fn set_verify(&mut self, on: bool) {
        self.verify = on;
    }

    pub fn ipc(&self) -> Option<&Client> {
        self.ipc.as_ref()
    }

    pub fn main_path(&self) -> &Path {
        self.cfg.main_path()
    }

    pub fn config(&self) -> &ConfigSet {
        &self.cfg
    }

    pub fn variables(&self) -> Vec<(String, String)> {
        self.cfg.variables()
    }

    // ---- single options -------------------------------------------------

    /// Value as written in the config, if any.
    pub fn value(&self, key: &str) -> Option<String> {
        self.cfg.get(key).map(|l| l.value)
    }

    /// Configured value, else the schema default.
    pub fn value_or_default(&self, opt: &Opt) -> String {
        self.value(&opt.key).unwrap_or_else(|| opt.default.clone())
    }

    /// File that defines `key` when it is not the main config.
    pub fn defined_elsewhere(&self, key: &str) -> Option<PathBuf> {
        let l = self.cfg.get(key)?;
        (l.file != 0).then(|| self.cfg.files()[l.file].to_path_buf())
    }

    /// Edits an option in the config and previews it live when possible.
    pub fn set_option(&mut self, key: &str, value: &str) -> Result<LiveStatus, Error> {
        let live = match self.schema.option(key) {
            Some(opt) => {
                if !opt.validate(value) {
                    return Err(Error::Invalid {
                        key: key.into(),
                        value: value.into(),
                    });
                }
                opt.live
            }
            None => true,
        };
        self.cfg.set(key, value);
        Ok(self.preview(key, value, live))
    }

    fn preview(&mut self, key: &str, value: &str, live: bool) -> LiveStatus {
        let Some(ipc) = &self.ipc else {
            return LiveStatus::NoIpc;
        };
        if !live {
            return LiveStatus::NotLive;
        }
        match ipc.keyword(key, value) {
            Ok(()) => {
                self.touched.insert(key.to_string());
                LiveStatus::Applied
            }
            Err(e) => LiveStatus::Failed(e.to_string()),
        }
    }

    /// Sends an arbitrary `keyword` live (monitors, binds…) without touching
    /// the config. Used for previews that are saved separately.
    pub fn preview_raw(&self, key: &str, value: &str) -> LiveStatus {
        match &self.ipc {
            None => LiveStatus::NoIpc,
            Some(c) => match c.keyword(key, value) {
                Ok(()) => LiveStatus::Applied,
                Err(e) => LiveStatus::Failed(e.to_string()),
            },
        }
    }

    // ---- repeatable keywords (bind, windowrule, exec-once, …) -----------

    pub fn list(&self, key: &str) -> Vec<Located> {
        self.cfg.get_all(key)
    }

    /// All top-level keywords whose name starts with `prefix` (`bind`, `bindl`, …).
    pub fn entries_with_prefix(&self, prefix: &str) -> Vec<Located> {
        self.cfg
            .entries()
            .into_iter()
            .filter(|e| !e.path.contains(':') && e.path.starts_with(prefix))
            .collect()
    }

    pub fn list_replace(&mut self, item: &Located, value: &str) -> bool {
        self.cfg.replace(item, value)
    }

    pub fn list_remove(&mut self, item: &Located) -> bool {
        self.cfg.remove(item)
    }

    pub fn list_push(&mut self, key: &str, value: &str) {
        self.cfg.push(key, value);
    }

    // ---- saving ----------------------------------------------------------

    pub fn is_dirty(&self) -> bool {
        self.cfg.is_dirty()
    }

    /// Writes changed files, checks the result with Hyprland, then reloads.
    /// If Hyprland rejects the config the files are restored.
    pub fn save(&mut self) -> Result<SaveReport, Error> {
        let saved = self.cfg.save()?;
        if self.verify {
            if let Err(msg) = verify_config(self.cfg.main_path()) {
                self.cfg.rollback(&saved);
                return Err(Error::Verify(msg));
            }
        }
        let reload_error = self
            .ipc
            .as_ref()
            .and_then(|c| c.reload().err())
            .map(|e| e.to_string());
        self.touched.clear();
        Ok(SaveReport {
            written: saved.into_iter().map(|s| s.path).collect(),
            reload_error,
        })
    }

    /// Discards unsaved edits and restores the live state.
    pub fn revert(&mut self) -> Result<(), Error> {
        let main = self.cfg.main_path().to_path_buf();
        let fresh = ConfigSet::load(&main)?;
        if let Some(ipc) = &self.ipc {
            let mut needs_reload = false;
            for key in &self.touched {
                match fresh.get(key) {
                    Some(l) => {
                        if ipc.keyword(key, &l.value).is_err() {
                            needs_reload = true;
                        }
                    }
                    None => needs_reload = true,
                }
            }
            if needs_reload {
                let _ = ipc.reload();
            }
        }
        self.touched.clear();
        self.cfg = fresh;
        Ok(())
    }
}

/// Runs `Hyprland --verify-config`. Passes when the binary is missing (so the
/// GUI works on machines without Hyprland) or reports no errors.
pub fn verify_config(path: &Path) -> Result<(), String> {
    for bin in ["Hyprland", "hyprland"] {
        let Ok(out) = Command::new(bin)
            .arg("--verify-config")
            .arg("--config")
            .arg(path)
            .output()
        else {
            continue;
        };
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let lower = text.to_lowercase();
        if lower.contains("config ok") {
            return Ok(());
        }
        if !out.status.success() || lower.contains("error") {
            return Err(text.trim().to_string());
        }
        return Ok(());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::sync::{Arc, Mutex};

    fn tmpdir(n: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("hyprgui-core-{n}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// Mock Hyprland socket that records commands and answers `ok`.
    fn mock_ipc(dir: &Path) -> (Client, Arc<Mutex<Vec<String>>>) {
        let path = dir.join("sock");
        let l = UnixListener::bind(&path).unwrap();
        let log = Arc::new(Mutex::new(Vec::new()));
        let log2 = log.clone();
        std::thread::spawn(move || {
            for c in l.incoming() {
                let Ok(mut c) = c else { break };
                let mut buf = [0u8; 1024];
                let n = c.read(&mut buf).unwrap_or(0);
                log2.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&buf[..n]).to_string());
                let _ = c.write_all(b"ok");
            }
        });
        (Client::at(path), log)
    }

    fn open(d: &Path, text: &str, ipc: Option<Client>) -> Session {
        fs::write(d.join("hyprland.conf"), text).unwrap();
        let mut s = Session::open(d.join("hyprland.conf"), ipc).unwrap();
        s.set_verify(false);
        s
    }

    #[test]
    fn set_option_edits_config_and_previews_live() {
        let d = tmpdir("live");
        let (ipc, log) = mock_ipc(&d);
        let mut s = open(&d, "general {\n    gaps_in = 5\n}\n", Some(ipc));
        assert_eq!(
            s.set_option("general:gaps_in", "9").unwrap(),
            LiveStatus::Applied
        );
        assert_eq!(s.value("general:gaps_in").as_deref(), Some("9"));
        assert!(s.is_dirty());
        assert_eq!(log.lock().unwrap()[0], "keyword general:gaps_in 9");
        fs::remove_dir_all(d).ok();
    }

    #[test]
    fn invalid_values_are_rejected_before_touching_anything() {
        let d = tmpdir("invalid");
        let mut s = open(&d, "", None);
        assert!(matches!(
            s.set_option("general:gaps_in", "-3"),
            Err(Error::Invalid { .. })
        ));
        assert!(!s.is_dirty());
        assert_eq!(
            s.set_option("general:gaps_in", "3").unwrap(),
            LiveStatus::NoIpc
        );
        fs::remove_dir_all(d).ok();
    }

    #[test]
    fn save_writes_and_reloads() {
        let d = tmpdir("save");
        let (ipc, log) = mock_ipc(&d);
        let mut s = open(
            &d,
            "# mine\ngeneral {\n    gaps_in = 5 # keep\n}\n",
            Some(ipc),
        );
        s.set_option("general:gaps_in", "7").unwrap();
        let report = s.save().unwrap();
        assert_eq!(report.written.len(), 1);
        assert_eq!(
            fs::read_to_string(d.join("hyprland.conf")).unwrap(),
            "# mine\ngeneral {\n    gaps_in = 7 # keep\n}\n"
        );
        assert!(log.lock().unwrap().iter().any(|c| c == "reload"));
        assert!(!s.is_dirty());
        fs::remove_dir_all(d).ok();
    }

    #[test]
    fn revert_restores_values_live() {
        let d = tmpdir("revert");
        let (ipc, log) = mock_ipc(&d);
        let mut s = open(&d, "general {\n    gaps_in = 5\n}\n", Some(ipc));
        s.set_option("general:gaps_in", "9").unwrap();
        s.revert().unwrap();
        assert_eq!(s.value("general:gaps_in").as_deref(), Some("5"));
        assert!(!s.is_dirty());
        assert_eq!(
            log.lock().unwrap().last().unwrap(),
            "keyword general:gaps_in 5"
        );
        fs::remove_dir_all(d).ok();
    }

    #[test]
    fn revert_of_newly_added_option_reloads_hyprland() {
        let d = tmpdir("revert-new");
        let (ipc, log) = mock_ipc(&d);
        let mut s = open(&d, "", Some(ipc));
        s.set_option("general:gaps_in", "9").unwrap();
        s.revert().unwrap();
        assert!(s.value("general:gaps_in").is_none());
        assert_eq!(log.lock().unwrap().last().unwrap(), "reload");
        fs::remove_dir_all(d).ok();
    }

    #[test]
    fn option_defined_in_include_is_edited_there() {
        let d = tmpdir("include");
        fs::write(d.join("theme.conf"), "general {\n    gaps_in = 2\n}\n").unwrap();
        let mut s = open(&d, "source = theme.conf\n", None);
        assert_eq!(
            s.defined_elsewhere("general:gaps_in"),
            Some(d.join("theme.conf"))
        );
        s.set_option("general:gaps_in", "4").unwrap();
        s.save().unwrap();
        assert!(fs::read_to_string(d.join("theme.conf"))
            .unwrap()
            .contains("gaps_in = 4"));
        assert_eq!(
            fs::read_to_string(d.join("hyprland.conf")).unwrap(),
            "source = theme.conf\n"
        );
        fs::remove_dir_all(d).ok();
    }

    #[test]
    fn lists_edit_and_prefix_scan() {
        let d = tmpdir("lists");
        let mut s = open(
            &d,
            "bind = A, a, exec, x\nbindl = , XF86Audio, exec, y\nbinds {\n    x = 1\n}\n",
            None,
        );
        assert_eq!(s.entries_with_prefix("bind").len(), 2);
        let item = s.list("bind")[0].clone();
        assert!(s.list_replace(&item, "A, a, exec, z"));
        s.list_push("exec-once", "waybar");
        assert_eq!(s.list("exec-once")[0].value, "waybar");
        fs::remove_dir_all(d).ok();
    }

    #[test]
    fn verify_passes_without_hyprland_binary() {
        // CI machines have no Hyprland; the check must not block saving.
        if Command::new("Hyprland").arg("--version").output().is_err()
            && Command::new("hyprland").arg("--version").output().is_err()
        {
            assert!(verify_config(Path::new("/nonexistent")).is_ok());
        }
    }
}
