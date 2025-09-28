use std::{
    fs,
    io::{self, copy},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use zip::ZipArchive;

use super::paths::RepoPaths;

const USER_AGENT: &str = "awesome-copilot-tui (+https://github.com/astrosteveo/awesome-copilot)";
const GITHUB_API: &str = "https://api.github.com";
const OWNER: &str = "github";
const REPO: &str = "awesome-copilot";
const REF: &str = "main";
const FRESHNESS_HOURS: i64 = 12;
const MAX_CACHE_ENTRIES: usize = 5;

#[derive(Debug, Clone)]
pub struct UpstreamSnapshot {
    pub commit: String,
    pub fetched_at: DateTime<Utc>,
    pub content_dir: PathBuf,
    pub warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CommitResponse {
    sha: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct SnapshotMetadata {
    commit: String,
    fetched_at: DateTime<Utc>,
}

pub fn ensure_snapshot(paths: &RepoPaths, force_refresh: bool) -> Result<UpstreamSnapshot> {
    paths
        .ensure_project_structure()
        .context("creating project directories")?;

    let client = Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(30))
        .build()
        .context("building HTTP client")?;

    let mut warnings = Vec::new();

    match fetch_latest_commit(&client) {
        Ok(commit) => {
            let snapshot_dir = paths.cache_dir.join(&commit);
            if !force_refresh {
                if let Some(snapshot) = try_load_snapshot(&snapshot_dir, false) {
                    return Ok(snapshot);
                }
            }

            match download_snapshot(&client, &paths.cache_dir, &commit) {
                Ok(snapshot) => {
                    prune_old_snapshots(&paths.cache_dir, MAX_CACHE_ENTRIES)?;
                    return Ok(snapshot);
                }
                Err(download_err) => {
                    warnings.push(format!(
                        "Failed to refresh upstream archive ({}); falling back to cached snapshot if available",
                        download_err
                    ));
                    if let Some(snapshot) = try_load_snapshot(&snapshot_dir, true) {
                        let mut snapshot = snapshot;
                        snapshot.warnings.extend(warnings);
                        return Ok(snapshot);
                    }
                }
            }

            if let Some(snapshot) = load_latest_snapshot(&paths.cache_dir) {
                let mut snapshot = snapshot?;
                snapshot.warnings.append(&mut warnings);
                snapshot
                    .warnings
                    .push("Using cached snapshot due to download failure".to_string());
                return Ok(snapshot);
            }

            Err(anyhow::anyhow!(
                "Unable to download upstream archive and no cache is available"
            ))
        }
        Err(err) => {
            warnings.push(format!(
                "Failed to query latest commit from GitHub API: {err}; attempting to use cached snapshot"
            ));
            if let Some(snapshot_result) = load_latest_snapshot(&paths.cache_dir) {
                let mut snapshot = snapshot_result?;
                snapshot.warnings.extend(warnings);
                return Ok(snapshot);
            }
            Err(anyhow::anyhow!(
                "Unable to determine latest commit and no cached snapshot exists"
            ))
        }
    }
}

fn fetch_latest_commit(client: &Client) -> Result<String> {
    let url = format!(
        "{GITHUB_API}/repos/{OWNER}/{REPO}/commits/{REF}",
        GITHUB_API = GITHUB_API,
        OWNER = OWNER,
        REPO = REPO,
        REF = REF
    );
    let response = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .context("fetching latest commit")?
        .error_for_status()
        .context("GitHub commit request failed")?;
    let commit: CommitResponse = response.json().context("parsing commit response")?;
    Ok(commit.sha)
}

fn download_snapshot(client: &Client, cache_dir: &Path, commit: &str) -> Result<UpstreamSnapshot> {
    let url = format!(
        "https://codeload.github.com/{OWNER}/{REPO}/zip/refs/heads/{REF}",
        OWNER = OWNER,
        REPO = REPO,
        REF = REF
    );
    let mut response = client
        .get(url)
        .send()
        .context("downloading upstream archive")?
        .error_for_status()
        .context("GitHub archive request failed")?;

    let mut tmp = NamedTempFile::new_in(cache_dir).context("creating temp file for archive")?;
    copy(&mut response, &mut tmp).context("writing archive to disk")?;

    let snapshot_dir = cache_dir.join(commit);
    if snapshot_dir.exists() {
        fs::remove_dir_all(&snapshot_dir)
            .with_context(|| format!("removing old snapshot at {}", snapshot_dir.display()))?;
    }
    fs::create_dir_all(&snapshot_dir)
        .with_context(|| format!("creating snapshot directory {}", snapshot_dir.display()))?;

    let file = tmp.reopen().context("reopening archive temp file")?;
    let mut archive = ZipArchive::new(file).context("opening archive")?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).context("reading archive entry")?;
        let outpath = snapshot_dir.join(entry.mangled_name());
        if entry.is_dir() {
            fs::create_dir_all(&outpath)
                .with_context(|| format!("creating directory {}", outpath.display()))?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("creating parent directory {}", parent.display()))?;
            }
            let mut outfile = fs::File::create(&outpath)
                .with_context(|| format!("writing file {}", outpath.display()))?;
            io::copy(&mut entry, &mut outfile)
                .with_context(|| format!("copying {}", outpath.display()))?;
        }
    }

    let content_dir = find_content_dir(&snapshot_dir)?;
    let fetched_at = Utc::now();
    let metadata = SnapshotMetadata {
        commit: commit.to_string(),
        fetched_at,
    };
    let metadata_path = snapshot_dir.join("snapshot.json");
    let metadata_file = fs::File::create(&metadata_path)
        .with_context(|| format!("writing metadata {}", metadata_path.display()))?;
    serde_json::to_writer_pretty(metadata_file, &metadata)
        .with_context(|| format!("serializing metadata {}", metadata_path.display()))?;

    Ok(UpstreamSnapshot {
        commit: commit.to_string(),
        fetched_at,
        content_dir,
        warnings: Vec::new(),
    })
}

fn find_content_dir(snapshot_dir: &Path) -> Result<PathBuf> {
    let mut entries = fs::read_dir(snapshot_dir)
        .with_context(|| format!("reading snapshot dir {}", snapshot_dir.display()))?;
    while let Some(entry) = entries.next() {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            return Ok(path);
        }
    }
    Err(anyhow::anyhow!(
        "snapshot did not contain a top-level directory"
    ))
}

fn try_load_snapshot(snapshot_dir: &Path, allow_stale: bool) -> Option<UpstreamSnapshot> {
    if !snapshot_dir.exists() {
        return None;
    }
    let metadata_path = snapshot_dir.join("snapshot.json");
    let metadata: SnapshotMetadata =
        serde_json::from_reader(fs::File::open(&metadata_path).ok()?).ok()?;
    let content_dir = find_content_dir(snapshot_dir).ok()?;
    let age_hours = Utc::now()
        .signed_duration_since(metadata.fetched_at)
        .num_hours();
    if !allow_stale && age_hours > FRESHNESS_HOURS {
        return None;
    }
    Some(UpstreamSnapshot {
        commit: metadata.commit,
        fetched_at: metadata.fetched_at,
        content_dir,
        warnings: Vec::new(),
    })
}

fn load_latest_snapshot(cache_dir: &Path) -> Option<Result<UpstreamSnapshot>> {
    let mut entries = match fs::read_dir(cache_dir) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
            .collect::<Vec<_>>(),
        Err(_) => return None,
    };
    entries
        .sort_by_key(|entry| std::cmp::Reverse(entry.metadata().and_then(|m| m.modified()).ok()));
    entries.into_iter().next().map(|entry| {
        try_load_snapshot(entry.path().as_path(), true).ok_or_else(|| {
            anyhow::anyhow!(
                "failed to load cached snapshot from {}",
                entry.path().display()
            )
        })
    })
}

fn prune_old_snapshots(cache_dir: &Path, keep: usize) -> Result<()> {
    let mut entries = fs::read_dir(cache_dir)
        .with_context(|| format!("reading cache dir {}", cache_dir.display()))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
        .collect::<Vec<_>>();

    entries.sort_by_key(|entry| {
        entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });

    while entries.len() > keep {
        if let Some(entry) = entries.first() {
            let path = entry.path();
            fs::remove_dir_all(&path)
                .with_context(|| format!("removing old snapshot {}", path.display()))?;
        }
        entries.remove(0);
    }
    Ok(())
}
