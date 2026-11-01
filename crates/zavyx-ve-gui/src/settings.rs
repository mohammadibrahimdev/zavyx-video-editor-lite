//! App settings kept in the data folder (Zavyx Standards §2):
//! `%LOCALAPPDATA%\mohammadibrahimdev\Zavyx VE\settings.json`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const MAX_RECENT: usize = 12;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RecentProject {
    pub path: PathBuf,
    pub name: String,
    /// Seconds since the Unix epoch of the last open/save.
    pub opened_at: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub recent: Vec<RecentProject>,
    #[serde(default)]
    pub last_export_dir: Option<PathBuf>,
}

/// `%LOCALAPPDATA%\mohammadibrahimdev\Zavyx VE` (created on demand).
pub fn data_dir() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share"))
    }?;
    let dir = base.join("mohammadibrahimdev").join("Zavyx VE");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Settings {
    pub fn load() -> Self {
        data_dir()
            .and_then(|d| std::fs::read_to_string(d.join("settings.json")).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let (Some(dir), Ok(text)) = (data_dir(), serde_json::to_string_pretty(self)) {
            let _ = std::fs::write(dir.join("settings.json"), text);
        }
    }

    /// Put a project at the front of the recent list.
    pub fn touch_recent(&mut self, path: PathBuf, name: String) {
        self.recent.retain(|r| r.path != path);
        self.recent.insert(0, RecentProject { path, name, opened_at: now_secs() });
        self.recent.truncate(MAX_RECENT);
    }

    /// Drop entries whose file no longer exists.
    pub fn prune_missing(&mut self) {
        self.recent.retain(|r| r.path.is_file());
    }
}

/// "Today", "Yesterday", or "N days ago" for a timestamp.
pub fn age_label(then: u64, now: u64) -> String {
    let days = now.saturating_sub(then) / 86_400;
    match days {
        0 => "Today".into(),
        1 => "Yesterday".into(),
        d => format!("{d} days ago"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_list_moves_to_front_and_caps() {
        let mut s = Settings::default();
        for i in 0..20 {
            s.touch_recent(PathBuf::from(format!("p{i}.zvep")), format!("P{i}"));
        }
        assert_eq!(s.recent.len(), MAX_RECENT);
        s.touch_recent(PathBuf::from("p10.zvep"), "P10".into());
        assert_eq!(s.recent[0].name, "P10");
        assert_eq!(s.recent.iter().filter(|r| r.name == "P10").count(), 1);
    }

    #[test]
    fn age_labels() {
        assert_eq!(age_label(100, 100), "Today");
        assert_eq!(age_label(0, 86_400 * 3 + 5), "3 days ago");
    }
}
