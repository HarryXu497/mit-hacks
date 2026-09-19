use crate::model::Session;
use crate::session::CoachingSession;
use anyhow::{Context, Result};
use bevy::prelude::*;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Resource, Debug, Default, Clone)]
pub struct PersistenceStatus {
    pub last_saved_path: Option<PathBuf>,
    pub error: Option<String>,
}

#[derive(Resource)]
pub struct AutosaveTracker {
    last_save: Instant,
    last_event_count: usize,
    last_title: String,
}

impl Default for AutosaveTracker {
    fn default() -> Self {
        Self {
            last_save: Instant::now() - Duration::from_secs(5),
            last_event_count: 0,
            last_title: String::new(),
        }
    }
}

pub fn recovery_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("TacticLab")
        .join("current-session.json")
}

pub fn load_recovery() -> Option<Session> {
    let bytes = fs::read(recovery_path()).ok()?;
    let mut session: Session = serde_json::from_slice(&bytes).ok()?;
    if session.validate_contract().is_err() {
        return None;
    }
    if matches!(
        session.status,
        crate::model::SessionStatus::Recording | crate::model::SessionStatus::Interpreted
    ) {
        session.status = crate::model::SessionStatus::Review;
    }
    Some(session)
}

pub fn autosave_session(
    session: Res<CoachingSession>,
    mut tracker: ResMut<AutosaveTracker>,
    mut status: ResMut<PersistenceStatus>,
) {
    let title_changed = tracker.last_title != session.session.title;
    let events_changed = tracker.last_event_count != session.session.events.len();
    let timed_recording_save = session.session.status == crate::model::SessionStatus::Recording
        && tracker.last_save.elapsed() >= Duration::from_secs(2);
    if !title_changed && !events_changed && !timed_recording_save {
        return;
    }

    let mut snapshot = session.session.clone();
    if snapshot.status == crate::model::SessionStatus::Recording {
        snapshot.elapsed_ms = session.elapsed_now();
    }
    let path = recovery_path();
    match atomic_json_write(&path, &snapshot) {
        Ok(()) => {
            tracker.last_event_count = snapshot.events.len();
            tracker.last_title.clone_from(&snapshot.title);
            tracker.last_save = Instant::now();
            status.error = None;
        }
        Err(error) => status.error = Some(format!("Autosave failed: {error:#}")),
    }
}

pub fn save_artifacts(session: &Session, output: &Value) -> Result<(PathBuf, PathBuf)> {
    session
        .validate_contract()
        .map_err(anyhow::Error::msg)
        .context("validating session contract")?;
    let directory = PathBuf::from("output")
        .join("sessions")
        .join(&session.id);
    fs::create_dir_all(&directory)
        .with_context(|| format!("creating {}", directory.display()))?;
    let session_path = directory.join("session.json");
    let tactics_path = directory.join("tactical-output.json");
    atomic_json_write(&session_path, session)?;
    atomic_json_write(&tactics_path, output)?;
    Ok((session_path, tactics_path))
}

pub fn export_tactical_json(session: &Session, output: &Value) -> Result<PathBuf> {
    session
        .validate_contract()
        .map_err(anyhow::Error::msg)
        .context("validating session contract")?;
    let directory = PathBuf::from("output")
        .join("sessions")
        .join(&session.id);
    fs::create_dir_all(&directory)
        .with_context(|| format!("creating {}", directory.display()))?;
    let path = directory.join("tactical-output.json");
    atomic_json_write(&path, output)?;
    Ok(path)
}

fn atomic_json_write(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let parent = path.parent().context("output path has no parent")?;
    fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    let temp = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().and_then(|name| name.to_str()).unwrap_or("data"),
        std::process::id()
    ));
    let bytes = serde_json::to_vec_pretty(value).context("serializing JSON")?;
    fs::write(&temp, bytes).with_context(|| format!("writing {}", temp.display()))?;

    #[cfg(windows)]
    if path.exists() {
        let backup = path.with_extension("json.bak");
        let _ = fs::remove_file(&backup);
        fs::rename(path, &backup).with_context(|| format!("backing up {}", path.display()))?;
        if let Err(error) = fs::rename(&temp, path) {
            let _ = fs::rename(&backup, path);
            return Err(error).with_context(|| format!("replacing {}", path.display()));
        }
        let _ = fs::remove_file(backup);
        return Ok(());
    }

    fs::rename(&temp, path).with_context(|| format!("replacing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_files_are_separate_and_valid() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested").join("session.json");
        atomic_json_write(&path, &Session::default()).unwrap();
        let loaded: Session = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(loaded.schema_version, 1);
    }
}
