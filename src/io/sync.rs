use std::{
    fs, io,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

use crate::domain::model::AssetKind;

use super::paths::RepoPaths;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalStatus {
    Missing,
    Same,
    Diff,
    NA, // Not applicable (e.g., collections)
}

#[derive(Debug, Clone)]
pub struct DiffEntry {
    pub kind: AssetKind,
    pub relative_path: String,
    pub status: LocalStatus,
}

pub fn compute_local_status(
    paths: &RepoPaths,
    upstream_root: &Path,
    kind: AssetKind,
    relative_path: &str,
) -> Result<LocalStatus> {
    if kind == AssetKind::Collection {
        return Ok(LocalStatus::NA);
    }
    let upstream_path = upstream_root.join(relative_path);
    let local_path = paths
        .asset_root(kind)
        .join(relative_path_for_kind(kind, relative_path));
    if !local_path.exists() {
        return Ok(LocalStatus::Missing);
    }
    let upstream_hash = hash_file(&upstream_path).context("hashing upstream file")?;
    let local_hash = hash_file(&local_path).context("hashing local file")?;
    if upstream_hash == local_hash {
        Ok(LocalStatus::Same)
    } else {
        Ok(LocalStatus::Diff)
    }
}

pub fn apply_from_upstream(
    paths: &RepoPaths,
    upstream_root: &Path,
    kind: AssetKind,
    relative_path: &str,
) -> Result<PathBuf> {
    if kind == AssetKind::Collection {
        // No-op: collections are not copied locally
        return Ok(paths.asset_root(kind).to_path_buf());
    }
    let upstream_path = upstream_root.join(relative_path);
    let local_relative = relative_path_for_kind(kind, relative_path);
    let local_path = paths.asset_root(kind).join(&local_relative);
    if let Some(parent) = local_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::copy(&upstream_path, &local_path).with_context(|| {
        format!(
            "copying {} -> {}",
            upstream_path.display(),
            local_path.display()
        )
    })?;
    Ok(local_path)
}

pub fn remove_local(paths: &RepoPaths, kind: AssetKind, relative_path: &str) -> Result<bool> {
    if kind == AssetKind::Collection {
        return Ok(false);
    }
    let local_relative = relative_path_for_kind(kind, relative_path);
    let local_path = paths.asset_root(kind).join(&local_relative);
    if local_path.exists() {
        std::fs::remove_file(&local_path)
            .with_context(|| format!("removing {}", local_path.display()))?;
        // Optionally clean up empty parent directories (best-effort)
        if let Some(parent) = local_path.parent() {
            let _ = std::fs::remove_dir(parent);
        }
        Ok(true)
    } else {
        Ok(false)
    }
}

fn relative_path_for_kind(_kind: AssetKind, relative_path: &str) -> PathBuf {
    // Upstream relative paths already start with prompts/, instructions/, chatmodes/, collections/
    // Our local roots are .github/<kind>, so drop the first segment.
    let mut comps = relative_path.split('/');
    comps.next(); // drop top-level kind dir
    PathBuf::from(comps.collect::<Vec<_>>().join("/"))
}

fn hash_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher).with_context(|| format!("hashing {}", path.display()))?;
    Ok(hex::encode(hasher.finalize()))
}
