use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::domain::model::{
    AssetKind, Catalog, ChatMode, Collection, CollectionItem, Instruction, Prompt,
};

use super::{paths::RepoPaths, upstream};

#[derive(Debug, Deserialize)]
struct FrontMatter {
    #[serde(default)]
    description: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    tools: Vec<String>,
    #[serde(default)]
    apply_to: String,
}

#[derive(Debug, Deserialize)]
struct CollectionYaml {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    items: Vec<CollectionItemYaml>,
}

#[derive(Debug, Deserialize)]
struct CollectionItemYaml {
    #[serde(default)]
    path: String,
    #[serde(default)]
    kind: String,
}

pub struct CatalogLoad {
    pub catalog: Catalog,
    pub warnings: Vec<String>,
    pub upstream_dir: PathBuf,
}

pub fn load_catalog(paths: &RepoPaths) -> Result<CatalogLoad> {
    let mut warnings = Vec::new();

    // Ensure upstream snapshot is available
    let snapshot =
        upstream::ensure_snapshot(paths, false).context("failed to obtain upstream snapshot")?;

    warnings.extend(snapshot.warnings);

    // Build catalog from upstream snapshot
    let catalog = build_catalog_from_snapshot(&snapshot.content_dir, &mut warnings)
        .context("failed to build catalog from upstream snapshot")?;

    Ok(CatalogLoad {
        catalog: catalog.finalize(),
        warnings,
        upstream_dir: snapshot.content_dir,
    })
}

fn build_catalog_from_snapshot(content_dir: &Path, warnings: &mut Vec<String>) -> Result<Catalog> {
    let mut catalog = Catalog::default();

    // Collect prompts
    catalog.prompts = collect_prompts(content_dir, warnings)?;

    // Collect instructions
    catalog.instructions = collect_instructions(content_dir, warnings)?;

    // Collect chat modes
    catalog.chat_modes = collect_chat_modes(content_dir, warnings)?;

    // Collect collections
    catalog.collections = collect_collections(content_dir, warnings)?;

    Ok(catalog)
}

fn collect_prompts(content_dir: &Path, warnings: &mut Vec<String>) -> Result<Vec<Prompt>> {
    let prompts_dir = content_dir.join("prompts");
    if !prompts_dir.exists() {
        return Ok(Vec::new());
    }

    let mut prompts = Vec::new();

    for entry in WalkDir::new(&prompts_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
        .filter(|e| e.file_name().to_string_lossy().ends_with(".prompt.md"))
    {
        match parse_prompt(entry.path(), content_dir) {
            Ok(prompt) => prompts.push(prompt),
            Err(err) => {
                warnings.push(format!(
                    "Failed to parse prompt {}: {}",
                    entry.path().display(),
                    err
                ));
            }
        }
    }

    prompts.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(prompts)
}

fn collect_instructions(
    content_dir: &Path,
    warnings: &mut Vec<String>,
) -> Result<Vec<Instruction>> {
    let instructions_dir = content_dir.join("instructions");
    if !instructions_dir.exists() {
        return Ok(Vec::new());
    }

    let mut instructions = Vec::new();

    for entry in WalkDir::new(&instructions_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .ends_with(".instructions.md")
        })
    {
        match parse_instruction(entry.path(), content_dir) {
            Ok(instruction) => instructions.push(instruction),
            Err(err) => {
                warnings.push(format!(
                    "Failed to parse instruction {}: {}",
                    entry.path().display(),
                    err
                ));
            }
        }
    }

    instructions.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(instructions)
}

fn collect_chat_modes(content_dir: &Path, warnings: &mut Vec<String>) -> Result<Vec<ChatMode>> {
    let chatmodes_dir = content_dir.join("chatmodes");
    if !chatmodes_dir.exists() {
        return Ok(Vec::new());
    }

    let mut chat_modes = Vec::new();

    for entry in WalkDir::new(&chatmodes_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
        .filter(|e| e.file_name().to_string_lossy().ends_with(".chatmode.md"))
    {
        match parse_chat_mode(entry.path(), content_dir) {
            Ok(chat_mode) => chat_modes.push(chat_mode),
            Err(err) => {
                warnings.push(format!(
                    "Failed to parse chat mode {}: {}",
                    entry.path().display(),
                    err
                ));
            }
        }
    }

    chat_modes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(chat_modes)
}

fn collect_collections(content_dir: &Path, warnings: &mut Vec<String>) -> Result<Vec<Collection>> {
    let collections_dir = content_dir.join("collections");
    if !collections_dir.exists() {
        return Ok(Vec::new());
    }

    let mut collections = Vec::new();

    for entry in WalkDir::new(&collections_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .map_or(false, |ext| ext == "yml" || ext == "yaml")
        })
        .filter(|e| e.file_name().to_string_lossy().ends_with(".collection.yml"))
    {
        match parse_collection(entry.path(), content_dir) {
            Ok(collection) => collections.push(collection),
            Err(err) => {
                warnings.push(format!(
                    "Failed to parse collection {}: {}",
                    entry.path().display(),
                    err
                ));
            }
        }
    }

    collections.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(collections)
}

fn parse_prompt(file_path: &Path, content_dir: &Path) -> Result<Prompt> {
    let content = fs::read_to_string(file_path)
        .with_context(|| format!("reading prompt file {}", file_path.display()))?;

    let relative_path = file_path
        .strip_prefix(content_dir)
        .with_context(|| format!("computing relative path for {}", file_path.display()))?
        .to_string_lossy()
        .to_string();

    let slug = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .replace(".prompt", "");

    let front_matter = parse_front_matter(&content)?;
    let name = extract_title(&content).unwrap_or_else(|| slug_to_title(&slug));
    let sha256 = compute_sha256(&content);

    Ok(Prompt {
        path: relative_path,
        slug,
        name,
        description: front_matter.description,
        mode: front_matter.mode,
        tags: front_matter.tags,
        sha256,
    })
}

fn parse_instruction(file_path: &Path, content_dir: &Path) -> Result<Instruction> {
    let content = fs::read_to_string(file_path)
        .with_context(|| format!("reading instruction file {}", file_path.display()))?;

    let relative_path = file_path
        .strip_prefix(content_dir)
        .with_context(|| format!("computing relative path for {}", file_path.display()))?
        .to_string_lossy()
        .to_string();

    let slug = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .replace(".instructions", "");

    let front_matter = parse_front_matter(&content)?;
    let name = extract_title(&content).unwrap_or_else(|| slug_to_title(&slug));
    let sha256 = compute_sha256(&content);

    let apply_to = if front_matter.apply_to.is_empty() {
        vec!["**".to_string()]
    } else {
        vec![front_matter.apply_to]
    };

    Ok(Instruction {
        path: relative_path,
        slug,
        name,
        description: front_matter.description,
        apply_to,
        tags: front_matter.tags,
        sha256,
    })
}

fn parse_chat_mode(file_path: &Path, content_dir: &Path) -> Result<ChatMode> {
    let content = fs::read_to_string(file_path)
        .with_context(|| format!("reading chat mode file {}", file_path.display()))?;

    let relative_path = file_path
        .strip_prefix(content_dir)
        .with_context(|| format!("computing relative path for {}", file_path.display()))?
        .to_string_lossy()
        .to_string();

    let slug = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .replace(".chatmode", "");

    let front_matter = parse_front_matter(&content)?;
    let name = extract_title(&content).unwrap_or_else(|| slug_to_title(&slug));
    let sha256 = compute_sha256(&content);

    Ok(ChatMode {
        path: relative_path,
        slug,
        name,
        description: front_matter.description,
        tools: front_matter.tools,
        tags: front_matter.tags,
        sha256,
    })
}

fn parse_collection(file_path: &Path, content_dir: &Path) -> Result<Collection> {
    let content = fs::read_to_string(file_path)
        .with_context(|| format!("reading collection file {}", file_path.display()))?;

    let relative_path = file_path
        .strip_prefix(content_dir)
        .with_context(|| format!("computing relative path for {}", file_path.display()))?
        .to_string_lossy()
        .to_string();

    let collection_yaml: CollectionYaml = serde_yaml::from_str(&content)
        .with_context(|| format!("parsing YAML in {}", file_path.display()))?;

    let slug = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .replace(".collection", "");

    let name = if collection_yaml.name.is_empty() {
        slug_to_title(&slug)
    } else {
        collection_yaml.name
    };

    let id = if collection_yaml.id.is_empty() {
        slug.clone()
    } else {
        collection_yaml.id
    };

    let items = collection_yaml
        .items
        .into_iter()
        .filter_map(|item| {
            let kind = match item.kind.as_str() {
                "prompt" => AssetKind::Prompt,
                "instruction" => AssetKind::Instruction,
                "chatmode" | "chat_mode" => AssetKind::ChatMode,
                "collection" => AssetKind::Collection,
                _ => return None,
            };
            Some(CollectionItem {
                path: item.path,
                kind,
            })
        })
        .collect();

    let sha256 = compute_sha256(&content);

    Ok(Collection {
        path: relative_path,
        id,
        slug,
        name,
        description: collection_yaml.description,
        tags: collection_yaml.tags,
        items,
        sha256,
    })
}

fn parse_front_matter(content: &str) -> Result<FrontMatter> {
    if !content.starts_with("---\n") {
        return Ok(FrontMatter::default());
    }

    let end_pos = content[4..]
        .find("\n---\n")
        .map(|pos| pos + 4)
        .unwrap_or_else(|| content.len());

    let front_matter_str = &content[4..end_pos];

    serde_yaml::from_str(front_matter_str).or_else(|_| Ok(FrontMatter::default()))
}

impl Default for FrontMatter {
    fn default() -> Self {
        Self {
            description: String::new(),
            tags: Vec::new(),
            mode: String::new(),
            tools: Vec::new(),
            apply_to: String::new(),
        }
    }
}

fn extract_title(content: &str) -> Option<String> {
    for line in content.lines() {
        if let Some(stripped) = line.strip_prefix("# ") {
            return Some(stripped.trim().to_string());
        }
    }
    None
}

fn slug_to_title(slug: &str) -> String {
    slug.split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn compute_sha256(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}
