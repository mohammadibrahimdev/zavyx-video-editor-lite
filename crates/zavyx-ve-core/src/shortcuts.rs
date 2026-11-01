//! Keyboard shortcut data model + editor.
//!
//! Stores a map of action_id → KeyboardShortcut. The GUI queries this map
//! to decide which key binding to display in menus and to listen for. The
//! user can edit the map via the shortcut editor dialog.
//!
//! Default bindings are sensible; the user's overrides are serialized to
//! `%LOCALAPPDATA%\mohammadibrahimdev\Zavyx VE\shortcuts.json`.

use std::collections::BTreeMap;
use serde::{Serialize, Deserialize};

/// A keyboard shortcut: modifiers + a single key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyboardShortcut {
    pub key: Key,
    pub modifiers: Modifiers,
}

/// Subset of egui::Key that we care about for shortcuts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Key {
    A, B, C, D, E, F, G, H, I, J, K, L, M,
    N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    Num0, Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9,
    Space, Enter, Escape, Tab, Backspace, Delete, Insert, Home, End,
    PageUp, PageDown, ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
}

/// Modifier flags (bitmask).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Modifiers {
    pub ctrl:  bool,
    pub shift: bool,
    pub alt:   bool,
    pub cmd:   bool,  // ⌘ on macOS, ignored on Windows/Linux
}

impl Modifiers {
    pub const NONE:  Modifiers = Modifiers { ctrl: false, shift: false, alt: false, cmd: false };
    pub const CTRL:  Modifiers = Modifiers { ctrl: true,  shift: false, alt: false, cmd: false };
    pub const SHIFT: Modifiers = Modifiers { ctrl: false, shift: true,  alt: false, cmd: false };
    pub const ALT:   Modifiers = Modifiers { ctrl: false, shift: false, alt: true,  cmd: false };
    pub const CTRL_SHIFT: Modifiers = Modifiers { ctrl: true, shift: true, alt: false, cmd: false };
}

impl std::fmt::Display for KeyboardShortcut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.modifiers.ctrl  { write!(f, "Ctrl+")?; }
        if self.modifiers.shift { write!(f, "Shift+")?; }
        if self.modifiers.alt   { write!(f, "Alt+")?; }
        if self.modifiers.cmd   { write!(f, "⌘+")?; }
        let key_str = match self.key {
            Key::A => "A", Key::B => "B", Key::C => "C", Key::D => "D",
            Key::E => "E", Key::F => "F", Key::G => "G", Key::H => "H",
            Key::I => "I", Key::J => "J", Key::K => "K", Key::L => "L",
            Key::M => "M", Key::N => "N", Key::O => "O", Key::P => "P",
            Key::Q => "Q", Key::R => "R", Key::S => "S", Key::T => "T",
            Key::U => "U", Key::V => "V", Key::W => "W", Key::X => "X",
            Key::Y => "Y", Key::Z => "Z",
            Key::Num0 => "0", Key::Num1 => "1", Key::Num2 => "2", Key::Num3 => "3",
            Key::Num4 => "4", Key::Num5 => "5", Key::Num6 => "6", Key::Num7 => "7",
            Key::Num8 => "8", Key::Num9 => "9",
            Key::Space => "Space", Key::Enter => "Enter", Key::Escape => "Esc",
            Key::Tab => "Tab", Key::Backspace => "Backspace", Key::Delete => "Del",
            Key::Insert => "Ins", Key::Home => "Home", Key::End => "End",
            Key::PageUp => "PgUp", Key::PageDown => "PgDn",
            Key::ArrowUp => "↑", Key::ArrowDown => "↓",
            Key::ArrowLeft => "←", Key::ArrowRight => "→",
            Key::F1 => "F1", Key::F2 => "F2", Key::F3 => "F3", Key::F4 => "F4",
            Key::F5 => "F5", Key::F6 => "F6", Key::F7 => "F7", Key::F8 => "F8",
            Key::F9 => "F9", Key::F10 => "F10", Key::F11 => "F11", Key::F12 => "F12",
        };
        write!(f, "{}", key_str)
    }
}

/// All editable shortcuts in the editor. Each entry is (action_id, description, default).
pub const DEFAULT_SHORTCUTS: &[(&str, &str, KeyboardShortcut)] = &[
    // File menu
    ("file.new",         "New Project",          KeyboardShortcut { key: Key::N, modifiers: Modifiers::CTRL }),
    ("file.open",        "Open Project...",       KeyboardShortcut { key: Key::O, modifiers: Modifiers::CTRL }),
    ("file.save",        "Save",                  KeyboardShortcut { key: Key::S, modifiers: Modifiers::CTRL }),
    ("file.save_as",     "Save As...",            KeyboardShortcut { key: Key::S, modifiers: Modifiers::CTRL_SHIFT }),
    ("file.export",      "Export MP4",            KeyboardShortcut { key: Key::E, modifiers: Modifiers::CTRL }),
    ("file.exit",        "Exit",                  KeyboardShortcut { key: Key::F4, modifiers: Modifiers { ctrl: true, shift: false, alt: true, cmd: false } }),
    // Edit menu
    ("edit.undo",        "Undo",                  KeyboardShortcut { key: Key::Z, modifiers: Modifiers::CTRL }),
    ("edit.redo",        "Redo",                  KeyboardShortcut { key: Key::Y, modifiers: Modifiers::CTRL }),
    ("edit.split",       "Split at Playhead",     KeyboardShortcut { key: Key::S, modifiers: Modifiers::NONE }),
    ("edit.delete_clip", "Delete Clip",           KeyboardShortcut { key: Key::Delete, modifiers: Modifiers::NONE }),
    ("edit.ripple_delete", "Ripple Delete",       KeyboardShortcut { key: Key::Delete, modifiers: Modifiers::SHIFT }),
    ("edit.copy",        "Copy",                  KeyboardShortcut { key: Key::C, modifiers: Modifiers::CTRL }),
    ("edit.paste",       "Paste",                 KeyboardShortcut { key: Key::V, modifiers: Modifiers::CTRL }),
    ("edit.cut",         "Cut",                   KeyboardShortcut { key: Key::X, modifiers: Modifiers::CTRL }),
    // View menu
    ("view.zoom_in",     "Zoom In",               KeyboardShortcut { key: Key::ArrowUp,   modifiers: Modifiers::CTRL }),
    ("view.zoom_out",    "Zoom Out",              KeyboardShortcut { key: Key::ArrowDown, modifiers: Modifiers::CTRL }),
    ("view.fit",         "Fit to Window",         KeyboardShortcut { key: Key::F, modifiers: Modifiers::NONE }),
    // Playback
    ("playback.play_pause", "Play / Pause",       KeyboardShortcut { key: Key::Space, modifiers: Modifiers::NONE }),
    ("playback.stop",      "Stop",                 KeyboardShortcut { key: Key::Space, modifiers: Modifiers::CTRL }),
    ("playback.skip_start", "Skip to Start",      KeyboardShortcut { key: Key::Home,  modifiers: Modifiers::NONE }),
    ("playback.skip_end",   "Skip to End",         KeyboardShortcut { key: Key::End,   modifiers: Modifiers::NONE }),
    ("playback.prev_frame", "Previous Frame",     KeyboardShortcut { key: Key::ArrowLeft, modifiers: Modifiers::NONE }),
    ("playback.next_frame", "Next Frame",          KeyboardShortcut { key: Key::ArrowRight, modifiers: Modifiers::NONE }),
    // Timeline
    ("timeline.add_marker", "Add Marker",         KeyboardShortcut { key: Key::M, modifiers: Modifiers::NONE }),
    // Tools
    ("tool.select",       "Select Tool",          KeyboardShortcut { key: Key::V, modifiers: Modifiers::NONE }),
    ("tool.blade",        "Blade Tool",            KeyboardShortcut { key: Key::B, modifiers: Modifiers::NONE }),
    ("tool.hand",         "Hand Tool",             KeyboardShortcut { key: Key::H, modifiers: Modifiers::NONE }),
];

/// The user's shortcut overrides. Maps action_id → shortcut.
/// Defaults are used for any action not present.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ShortcutMap {
    pub overrides: BTreeMap<String, KeyboardShortcut>,
}

impl ShortcutMap {
    /// Look up the shortcut for an action. Falls back to the default if
    /// no override exists.
    pub fn get(&self, action_id: &str) -> Option<KeyboardShortcut> {
        if let Some(s) = self.overrides.get(action_id) {
            Some(*s)
        } else {
            DEFAULT_SHORTCUTS.iter()
                .find(|(id, _, _)| *id == action_id)
                .map(|(_, _, s)| *s)
        }
    }

    /// Set an override for an action.
    pub fn set(&mut self, action_id: impl Into<String>, shortcut: KeyboardShortcut) {
        self.overrides.insert(action_id.into(), shortcut);
    }

    /// Reset all overrides to defaults.
    pub fn reset_to_defaults(&mut self) {
        self.overrides.clear();
    }

    /// Returns true if any overrides differ from the defaults.
    pub fn is_modified(&self) -> bool {
        !self.overrides.is_empty()
    }

    /// Serialize to JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize from JSON.
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_display_includes_modifiers() {
        let s = KeyboardShortcut { key: Key::S, modifiers: Modifiers::CTRL };
        assert_eq!(s.to_string(), "Ctrl+S");
        let s = KeyboardShortcut { key: Key::Z, modifiers: Modifiers::CTRL_SHIFT };
        assert_eq!(s.to_string(), "Ctrl+Shift+Z");
        let s = KeyboardShortcut { key: Key::Space, modifiers: Modifiers::NONE };
        assert_eq!(s.to_string(), "Space");
    }

    #[test]
    fn shortcut_map_returns_default_for_known_action() {
        let m = ShortcutMap::default();
        let s = m.get("file.save");
        assert_eq!(s, Some(KeyboardShortcut { key: Key::S, modifiers: Modifiers::CTRL }));
    }

    #[test]
    fn shortcut_map_override_takes_precedence() {
        let mut m = ShortcutMap::default();
        m.set("file.save", KeyboardShortcut { key: Key::Q, modifiers: Modifiers::CTRL });
        let s = m.get("file.save");
        assert_eq!(s, Some(KeyboardShortcut { key: Key::Q, modifiers: Modifiers::CTRL }));
    }

    #[test]
    fn shortcut_map_returns_none_for_unknown_action() {
        let m = ShortcutMap::default();
        assert!(m.get("nonexistent.action").is_none());
    }

    #[test]
    fn shortcut_map_reset_clears_overrides() {
        let mut m = ShortcutMap::default();
        m.set("file.save", KeyboardShortcut { key: Key::Q, modifiers: Modifiers::CTRL });
        assert!(m.is_modified());
        m.reset_to_defaults();
        assert!(!m.is_modified());
    }

    #[test]
    fn shortcut_map_round_trips_json() {
        let mut m = ShortcutMap::default();
        m.set("file.save", KeyboardShortcut { key: Key::Q, modifiers: Modifiers::CTRL });
        let json = m.to_json().unwrap();
        let back = ShortcutMap::from_json(&json).unwrap();
        assert_eq!(back.get("file.save"), Some(KeyboardShortcut { key: Key::Q, modifiers: Modifiers::CTRL }));
    }

    #[test]
    fn default_shortcuts_cover_all_main_actions() {
        // The default table should have at least 25 actions.
        assert!(DEFAULT_SHORTCUTS.len() >= 25);
        // Sanity check: file.save should exist.
        assert!(DEFAULT_SHORTCUTS.iter().any(|(id, _, _)| *id == "file.save"));
        // playback.play_pause should be Space.
        let play_pause = DEFAULT_SHORTCUTS.iter().find(|(id, _, _)| *id == "playback.play_pause");
        assert_eq!(play_pause.unwrap().2.key, Key::Space);
    }
}
