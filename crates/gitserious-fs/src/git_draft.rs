use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;

use gitserious_app::{DraftStore, RepositoryRoot, SavedDraft, SavedProperty};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

/// Keeps the draft at `git rev-parse --git-path gitserious/draft.toml`, so each work tree has
/// its own and Git never commits it.
#[derive(Clone, Copy, Debug, Default)]
pub struct GitDraftStore;

impl DraftStore for GitDraftStore {
    fn load(&self, root: &RepositoryRoot) -> Result<Option<SavedDraft>, String> {
        let path = draft_path(root)?;
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("could not read {}: {error}", path.display())),
        };
        let wire: DraftWire = toml::from_str(&text)
            .map_err(|error| format!("invalid draft {}: {}", path.display(), error.message()))?;
        Ok(Some(wire.into()))
    }

    fn save(&self, root: &RepositoryRoot, draft: &SavedDraft) -> Result<(), String> {
        let path = draft_path(root)?;
        let directory = path.parent().ok_or("the draft path has no directory")?;
        fs::create_dir_all(directory)
            .map_err(|error| format!("could not create {}: {error}", directory.display()))?;
        let text = toml::to_string(&DraftWire::from(draft))
            .map_err(|error| format!("could not serialize the draft: {error}"))?;
        // A rename replaces the old draft whole, so a crash never leaves half of one.
        let mut file = NamedTempFile::new_in(directory)
            .map_err(|error| format!("could not create a draft file: {error}"))?;
        file.write_all(text.as_bytes())
            .map_err(|error| format!("could not write the draft: {error}"))?;
        file.persist(&path)
            .map(|_| ())
            .map_err(|error| format!("could not save {}: {}", path.display(), error.error))
    }

    fn discard(&self, root: &RepositoryRoot) -> Result<(), String> {
        let path = draft_path(root)?;
        match fs::remove_file(&path) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => {
                Err(format!("could not delete {}: {error}", path.display()))
            }
            _ => Ok(()),
        }
    }
}

/// Where Git puts per-work-tree state named `gitserious/draft.toml`.
fn draft_path(root: &RepositoryRoot) -> Result<PathBuf, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root.as_path())
        .args(["rev-parse", "--git-path", "gitserious/draft.toml"])
        .output()
        .map_err(|error| format!("could not execute git rev-parse: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    let path = String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_owned();
    Ok(root.as_path().join(path))
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct DraftWire {
    taxonomy: String,
    fingerprint: String,
    commit_type: String,
    subject: String,
    breaking_change: String,
    #[serde(default)]
    properties: Vec<PropertyWire>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct PropertyWire {
    key: String,
    text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    applies: Option<bool>,
}

impl From<&SavedDraft> for DraftWire {
    fn from(draft: &SavedDraft) -> Self {
        Self {
            taxonomy: draft.taxonomy.clone(),
            fingerprint: draft.fingerprint.clone(),
            commit_type: draft.commit_type.clone(),
            subject: draft.subject.clone(),
            breaking_change: draft.breaking_change.clone(),
            properties: draft
                .properties
                .iter()
                .map(|property| PropertyWire {
                    key: property.key.clone(),
                    text: property.text.clone(),
                    applies: property.applies,
                })
                .collect(),
        }
    }
}

impl From<DraftWire> for SavedDraft {
    fn from(wire: DraftWire) -> Self {
        Self {
            taxonomy: wire.taxonomy,
            fingerprint: wire.fingerprint,
            commit_type: wire.commit_type,
            subject: wire.subject,
            breaking_change: wire.breaking_change,
            properties: wire
                .properties
                .into_iter()
                .map(|property| SavedProperty {
                    key: property.key,
                    text: property.text,
                    applies: property.applies,
                })
                .collect(),
        }
    }
}
