use anyhow::{anyhow, Context, Result};
use std::{fs, path::Path};

use serde::de::{self, Deserializer};
use serde::Deserialize;

use crate::domain::model::{
    AssetKind, Catalog, ChatMode, Collection, CollectionItem, Instruction, Prompt,
};

use super::paths::RepoPaths;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCatalog {
    #[serde(default)]
    generated_at: Option<String>,
    #[serde(default)]
    prompts: Vec<RawPrompt>,
    #[serde(default)]
    instructions: Vec<RawInstruction>,
    #[serde(default, rename = "chatModes")]
    chat_modes: Vec<RawChatMode>,
    #[serde(default)]
    collections: Vec<RawCollection>,
}

#[derive(Debug, Deserialize)]
struct RawPrompt {
    path: String,
    slug: String,
    name: String,
    description: String,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawInstruction {
    path: String,
    slug: String,
    name: String,
    description: String,
    #[serde(default, deserialize_with = "deserialize_apply_to")]
    apply_to: Vec<String>,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawChatMode {
    path: String,
    slug: String,
    name: String,
    description: String,
    #[serde(default)]
    tools: Vec<String>,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawCollection {
    path: String,
    id: String,
    name: String,
    description: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    items: Vec<RawCollectionItem>,
}

#[derive(Debug, Deserialize)]
struct RawCollectionItem {
    path: String,
    kind: String,
}

impl From<RawPrompt> for Prompt {
    fn from(raw: RawPrompt) -> Self {
        Self {
            path: raw.path,
            slug: raw.slug,
            name: raw.name,
            description: raw.description,
            mode: raw.mode,
            tags: raw.tags,
        }
    }
}

impl From<RawInstruction> for Instruction {
    fn from(raw: RawInstruction) -> Self {
        Self {
            path: raw.path,
            slug: raw.slug,
            name: raw.name,
            description: raw.description,
            apply_to: raw.apply_to,
            tags: raw.tags,
        }
    }
}

impl From<RawChatMode> for ChatMode {
    fn from(raw: RawChatMode) -> Self {
        Self {
            path: raw.path,
            slug: raw.slug,
            name: raw.name,
            description: raw.description,
            tools: raw.tools,
            tags: raw.tags,
        }
    }
}

fn deserialize_apply_to<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Null => Ok(Vec::new()),
        serde_json::Value::String(s) => Ok(vec![s]),
        serde_json::Value::Array(values) => values
            .into_iter()
            .map(|item| match item {
                serde_json::Value::String(s) => Ok(s),
                other => Err(de::Error::custom(format!(
                    "applyTo entries must be strings, found {other:?}"
                ))),
            })
            .collect(),
        other => Err(de::Error::custom(format!(
            "applyTo must be a string or array of strings, found {other:?}"
        ))),
    }
}

impl TryFrom<RawCollectionItem> for CollectionItem {
    type Error = anyhow::Error;

    fn try_from(value: RawCollectionItem) -> Result<Self> {
        let kind = match value.kind.as_str() {
            "prompt" => AssetKind::Prompt,
            "instruction" => AssetKind::Instruction,
            "chat-mode" | "chatMode" => AssetKind::ChatMode,
            other => {
                return Err(anyhow!(
                    "Unsupported collection item kind '{}' for path {}",
                    other,
                    value.path
                ))
            }
        };
        Ok(Self {
            path: value.path,
            kind,
        })
    }
}

impl TryFrom<RawCollection> for Collection {
    type Error = anyhow::Error;

    fn try_from(value: RawCollection) -> Result<Self> {
        let items = value
            .items
            .into_iter()
            .map(CollectionItem::try_from)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            path: value.path,
            id: value.id,
            name: value.name,
            description: value.description,
            tags: value.tags,
            items,
        })
    }
}

#[derive(Debug, Clone)]
pub struct CatalogLoad {
    pub catalog: Catalog,
    pub warnings: Vec<String>,
}

pub fn load_catalog(paths: &RepoPaths) -> Result<CatalogLoad> {
    let content = match fs::read_to_string(&paths.metadata) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(CatalogLoad {
                catalog: Catalog::default(),
                warnings: vec![format!(
                    "asset metadata missing at {}; run `node scripts/export-asset-metadata.js` and try again",
                    paths.metadata.display()
                )],
            })
        }
        Err(err) => {
            return Err(err).with_context(|| {
                format!(
                    "Failed to read asset metadata from {}",
                    paths.metadata.display()
                )
            });
        }
    };

    let raw: RawCatalog = match serde_json::from_str(&content) {
        Ok(raw) => raw,
        Err(err) => {
            return Ok(CatalogLoad {
                catalog: Catalog::default(),
                warnings: vec![format!(
                    "asset metadata parse error; using empty catalog: {}",
                    err
                )],
            })
        }
    };

    let prompts = raw.prompts.into_iter().map(Prompt::from).collect();
    let instructions = raw
        .instructions
        .into_iter()
        .map(Instruction::from)
        .collect();
    let chat_modes = raw.chat_modes.into_iter().map(ChatMode::from).collect();
    let collections = raw
        .collections
        .into_iter()
        .map(Collection::try_from)
        .collect::<Result<Vec<_>>>()?;

    Ok(CatalogLoad {
        catalog: Catalog {
            prompts,
            instructions,
            chat_modes,
            collections,
            ..Catalog::default()
        },
        warnings: Vec::new(),
    })
}

pub fn load_catalog_from_path(path: &Path) -> Result<CatalogLoad> {
    let paths = RepoPaths::new(path);
    load_catalog(&paths)
}
