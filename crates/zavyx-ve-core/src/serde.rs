//! Project serialization and versioned migration.
//!
//! `.zvep` files are JSON. The top-level object is:
//!
//! ```json
//! {
//!   "schema_version": 2,
//!   "project": { ... Project ... }
//! }
//! ```
//!
//! On load, if `schema_version` is older than the current version, the
//! registered migrations run in order: v1 -> v2 -> ... -> current.

use serde::{Serialize, Deserialize};
use serde_json::{Value, json};
use crate::project::Project;
use thiserror::Error;

/// Current schema version. Bump on every breaking change to the project
/// model. Add a migration in [`migrations`] for each step.
pub const SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Error)]
pub enum LoadError {
    #[error("invalid .zvep file: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("missing schema_version field")]
    MissingVersion,
    #[error("unsupported schema version {found}; this build supports up to {supported}")]
    UnsupportedVersion { found: u32, supported: u32 },
    #[error("migration {0}->{1} failed: {2}")]
    MigrationFailed(u32, u32, String),
}

/// Save a project to a JSON string suitable for a `.zvep` file.
pub fn save(project: &Project) -> Result<String, serde_json::Error> {
    let wrapper = json!({
        "schema_version": SCHEMA_VERSION,
        "project": project,
    });
    serde_json::to_string_pretty(&wrapper)
}

/// Load a project from a `.zvep` JSON string. Runs migrations if needed.
pub fn load(text: &str) -> Result<Project, LoadError> {
    let value: Value = serde_json::from_str(text)?;
    let version = value.get("schema_version")
        .and_then(|v| v.as_u64())
        .ok_or(LoadError::MissingVersion)? as u32;
    if version > SCHEMA_VERSION {
        return Err(LoadError::UnsupportedVersion { found: version, supported: SCHEMA_VERSION });
    }
    let mut project_value = value.get("project").cloned().unwrap_or(Value::Null);
    for from in version..SCHEMA_VERSION {
        let to = from + 1;
        project_value = migrations::apply(from, to, project_value)?;
    }
    let project: Project = serde_json::from_value(project_value)?;
    // Force the project's internal `version` field to the latest schema version
    // in case the loaded value was stale.
    let mut project = project;
    project.version = SCHEMA_VERSION;
    Ok(project)
}

/// Module holding per-step migration functions.
pub mod migrations {
    use serde_json::Value;
    use super::LoadError;

    /// Apply the migration `from` -> `to`. Currently only `1 -> 2` exists.
    pub fn apply(from: u32, to: u32, mut value: Value) -> Result<Value, LoadError> {
        match (from, to) {
            (1, 2) => migrate_v1_to_v2(&mut value).map_err(|e| LoadError::MigrationFailed(1, 2, e))?,
            (_, _) if from == to => {} // no-op
            _ => return Err(LoadError::MigrationFailed(from, to, "no such migration".into())),
        }
        Ok(value)
    }

    /// v1 -> v2: introduce the `version` field on `Project` if missing,
    /// and rename `aspect_ratio` -> `aspect` in `Settings`.
    fn migrate_v1_to_v2(value: &mut Value) -> Result<(), String> {
        // Ensure version field exists and is set to 2.
        let proj = value.as_object_mut().ok_or("project is not an object")?;
        proj.entry("version").or_insert(Value::from(2u32));

        // Rename settings.aspect_ratio -> settings.aspect (v1 had the old name).
        if let Some(settings) = proj.get_mut("settings").and_then(|s| s.as_object_mut()) {
            if let Some(ar) = settings.remove("aspect_ratio") {
                settings.entry("aspect").or_insert(ar);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Project;

    #[test]
    fn save_load_round_trip() {
        let mut p = Project::new("Round Trip");
        // Add some media + a clip so the project isn't empty.
        let media = crate::MediaRef {
            id: crate::MediaId::new(),
            relative_path: std::path::PathBuf::from("clip.mp4"),
            display_name: "Clip".into(),
        };
        let info = crate::MediaInfo {
            path: std::path::PathBuf::from("clip.mp4"),
            duration: crate::Rational::new(60, 1),
            width: Some(1920), height: Some(1080),
            fps: Some(crate::FrameRate::FPS_30),
            video_codec: Some("h264".into()),
            audio_codec: Some("aac".into()),
            sample_rate: Some(48000), channels: Some(2),
            has_audio: true, has_video: true, file_size: 1_000_000,
        };
        let _ = p.media_pool.add(media, info);

        let json = save(&p).unwrap();
        let loaded = load(&json).unwrap();
        assert_eq!(loaded.name, "Round Trip");
        assert_eq!(loaded.version, SCHEMA_VERSION);
        assert_eq!(loaded.media_pool.items.len(), 1);
    }

    #[test]
    fn empty_project_round_trip() {
        let p = Project::new("Empty");
        let json = save(&p).unwrap();
        let loaded = load(&json).unwrap();
        assert_eq!(loaded.name, "Empty");
        assert!(loaded.sequences.len() >= 1);
    }

    #[test]
    fn load_v1_migrates_to_v2() {
        // A minimal v1 project: no "version" field on Project, "aspect_ratio"
        // (the v1 name) instead of "aspect" in Settings.
        let v1 = r#"{
            "schema_version": 1,
            "project": {
                "id": "00000000-0000-0000-0000-000000000000",
                "name": "Old",
                "settings": {
                    "time_base": [30, 1],
                    "aspect_ratio": "Landscape",
                    "width": 1920, "height": 1080,
                    "sample_rate": 48000, "audio_channels": 2,
                    "working_color_space": "linear-rec709"
                },
                "media_pool": {"items": {}},
                "sequences": {},
                "active_sequence": null
            }
        }"#;
        let p = load(v1).unwrap();
        assert_eq!(p.version, 2);
    }

    #[test]
    fn load_rejects_future_version() {
        let future = format!(r#"{{"schema_version": {}, "project": {{}}}}"#, SCHEMA_VERSION + 1);
        let err = load(&future).unwrap_err();
        assert!(matches!(err, LoadError::UnsupportedVersion { .. }));
    }

    #[test]
    fn load_rejects_missing_version() {
        let no_ver = r#"{"project": {}}"#;
        let err = load(no_ver).unwrap_err();
        assert!(matches!(err, LoadError::MissingVersion));
    }
}
