//! Minimal client for Hyprland's request socket (`.socket.sock`).

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Hyprland: {0}")]
    Hyprland(String),
    #[error("bad JSON from Hyprland: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
pub struct Client {
    socket: PathBuf,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Monitor {
    #[serde(default)]
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub make: String,
    #[serde(default)]
    pub model: String,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub refresh_rate: f64,
    pub x: i32,
    pub y: i32,
    #[serde(default = "one")]
    pub scale: f64,
    #[serde(default)]
    pub transform: u32,
    #[serde(default)]
    pub focused: bool,
    #[serde(default)]
    pub disabled: bool,
    /// Like `1920x1080@60.00Hz`.
    #[serde(default)]
    pub available_modes: Vec<String>,
}

fn one() -> f64 {
    1.0
}

#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    #[serde(default)]
    pub class: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub initial_class: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
pub struct Keyboard {
    pub name: String,
    #[serde(default)]
    pub layout: String,
    #[serde(default)]
    pub main: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct Devices {
    #[serde(default)]
    keyboards: Vec<Keyboard>,
}

impl Client {
    /// Socket from `$HYPRLAND_INSTANCE_SIGNATURE` and `$XDG_RUNTIME_DIR`.
    pub fn from_env() -> Option<Client> {
        let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
        let path = Path::new(&runtime)
            .join("hypr")
            .join(sig)
            .join(".socket.sock");
        path.exists().then_some(Client { socket: path })
    }

    pub fn at(socket: impl Into<PathBuf>) -> Client {
        Client {
            socket: socket.into(),
        }
    }

    /// Sends a raw command and returns the reply text.
    pub fn request(&self, command: &str) -> Result<String, Error> {
        let mut s = UnixStream::connect(&self.socket)?;
        s.set_read_timeout(Some(Duration::from_secs(3)))?;
        s.set_write_timeout(Some(Duration::from_secs(3)))?;
        s.write_all(command.as_bytes())?;
        let mut out = String::new();
        s.read_to_string(&mut out)?;
        Ok(out)
    }

    fn expect_ok(&self, command: &str) -> Result<(), Error> {
        let r = self.request(command)?;
        if r.trim() == "ok" {
            Ok(())
        } else {
            Err(Error::Hyprland(r.trim().to_string()))
        }
    }

    /// `hyprctl keyword <key> <value>`: applies a setting live, not persisted.
    pub fn keyword(&self, key: &str, value: &str) -> Result<(), Error> {
        self.expect_ok(&format!("keyword {key} {value}"))
    }

    pub fn reload(&self) -> Result<(), Error> {
        self.expect_ok("reload")
    }

    pub fn monitors(&self) -> Result<Vec<Monitor>, Error> {
        Ok(serde_json::from_str(&self.request("j/monitors all")?)?)
    }

    pub fn windows(&self) -> Result<Vec<Window>, Error> {
        Ok(serde_json::from_str(&self.request("j/clients")?)?)
    }

    pub fn keyboards(&self) -> Result<Vec<Keyboard>, Error> {
        let d: Devices = serde_json::from_str(&self.request("j/devices")?)?;
        Ok(d.keyboards)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    /// One-shot mock: answers each connection with the matching canned reply.
    fn mock(
        name: &str,
        replies: Vec<(&'static str, &'static str)>,
    ) -> (Client, std::thread::JoinHandle<Vec<String>>) {
        let path = std::env::temp_dir().join(format!("hypripc-{name}-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let l = UnixListener::bind(&path).unwrap();
        let n = replies.len();
        let h = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for _ in 0..n {
                let (mut c, _) = l.accept().unwrap();
                let mut buf = [0u8; 1024];
                let len = c.read(&mut buf).unwrap();
                let cmd = String::from_utf8_lossy(&buf[..len]).to_string();
                let reply = replies
                    .iter()
                    .find(|(p, _)| cmd.starts_with(p))
                    .map_or("unknown request", |r| r.1);
                c.write_all(reply.as_bytes()).unwrap();
                seen.push(cmd);
            }
            seen
        });
        (Client::at(path), h)
    }

    #[test]
    fn keyword_ok_and_error() {
        let (c, h) = mock(
            "kw",
            vec![
                ("keyword general:gaps_in 5", "ok"),
                ("keyword bad", "no such option"),
            ],
        );
        assert!(c.keyword("general:gaps_in", "5").is_ok());
        let e = c.keyword("bad", "1").unwrap_err();
        assert!(matches!(e, Error::Hyprland(m) if m == "no such option"));
        assert_eq!(h.join().unwrap()[0], "keyword general:gaps_in 5");
    }

    #[test]
    fn parses_monitors() {
        let json = r#"[{"id":0,"name":"DP-1","description":"X","make":"A","model":"B","width":2560,"height":1440,
          "refreshRate":143.97,"x":0,"y":0,"scale":1.25,"transform":0,"focused":true,"disabled":false,
          "availableModes":["2560x1440@143.97Hz","1920x1080@60.00Hz"],"activeWorkspace":{"id":1}}]"#;
        let (c, h) = mock(
            "mon",
            vec![(
                "j/monitors all",
                Box::leak(json.to_string().into_boxed_str()),
            )],
        );
        let m = c.monitors().unwrap();
        assert_eq!(m[0].name, "DP-1");
        assert_eq!(m[0].available_modes.len(), 2);
        assert!((m[0].scale - 1.25).abs() < 1e-9);
        h.join().unwrap();
    }

    #[test]
    fn parses_windows_and_keyboards() {
        let (c, h) = mock(
            "win",
            vec![
                (
                    "j/clients",
                    r#"[{"class":"kitty","title":"t","initialClass":"kitty","pid":1}]"#,
                ),
                (
                    "j/devices",
                    r#"{"mice":[],"keyboards":[{"name":"kbd","layout":"us,cz","main":true}]}"#,
                ),
            ],
        );
        assert_eq!(c.windows().unwrap()[0].class, "kitty");
        assert_eq!(c.keyboards().unwrap()[0].layout, "us,cz");
        h.join().unwrap();
    }

    #[test]
    fn missing_socket_is_io_error() {
        assert!(matches!(
            Client::at("/nonexistent/sock").reload(),
            Err(Error::Io(_))
        ));
    }
}
