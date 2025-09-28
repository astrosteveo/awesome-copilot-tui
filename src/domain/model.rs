use chrono::{DateTime, Utc};
use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Ord, PartialOrd)]
#[serde(rename_all = "camelCase")]
pub enum AssetKind {
    Prompt,
    Instruction,
    ChatMode,
    Collection,
}

#[derive(Debug, Clone)]
pub struct Prompt {
    pub path: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub mode: String,
    pub tags: Vec<String>,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub struct Instruction {
    pub path: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub apply_to: Vec<String>,
    pub tags: Vec<String>,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub struct ChatMode {
    pub path: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub tools: Vec<String>,
    pub tags: Vec<String>,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub struct CollectionItem {
    pub path: String,
    pub kind: AssetKind,
}

#[derive(Debug, Clone)]
pub struct Collection {
    pub path: String,
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub items: Vec<CollectionItem>,
    pub sha256: String,
}

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub prompts: Vec<Prompt>,
    pub instructions: Vec<Instruction>,
    pub chat_modes: Vec<ChatMode>,
    pub collections: Vec<Collection>,
    pub prompt_index: HashSet<String>,
    pub instruction_index: HashSet<String>,
    pub chat_mode_index: HashSet<String>,
    pub collection_index: HashSet<String>,
    pub collection_lookup: HashMap<String, Collection>,
    pub membership: HashMap<String, Vec<String>>, // asset path -> collection ids
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EnablementFile {
    pub version: u32,
    pub updated_at: Option<DateTime<Utc>>,
    pub prompts: BTreeMap<String, bool>,
    pub instructions: BTreeMap<String, bool>,
    pub chat_modes: BTreeMap<String, bool>,
    pub collections: BTreeMap<String, bool>,
    pub overrides: serde_json::Map<String, serde_json::Value>,
}

impl Default for EnablementFile {
    fn default() -> Self {
        Self {
            version: 1,
            updated_at: None,
            prompts: BTreeMap::new(),
            instructions: BTreeMap::new(),
            chat_modes: BTreeMap::new(),
            collections: BTreeMap::new(),
            overrides: serde_json::Map::new(),
        }
    }
}

impl EnablementFile {
    pub fn map_for(&self, kind: AssetKind) -> &BTreeMap<String, bool> {
        match kind {
            AssetKind::Prompt => &self.prompts,
            AssetKind::Instruction => &self.instructions,
            AssetKind::ChatMode => &self.chat_modes,
            AssetKind::Collection => &self.collections,
        }
    }

    pub fn map_for_mut(&mut self, kind: AssetKind) -> &mut BTreeMap<String, bool> {
        match kind {
            AssetKind::Prompt => &mut self.prompts,
            AssetKind::Instruction => &mut self.instructions,
            AssetKind::ChatMode => &mut self.chat_modes,
            AssetKind::Collection => &mut self.collections,
        }
    }

    pub fn remove(&mut self, kind: AssetKind, path: &str) {
        self.map_for_mut(kind).remove(path);
    }

    pub fn set(&mut self, kind: AssetKind, path: &str, value: bool) {
        self.map_for_mut(kind).insert(path.to_string(), value);
    }
}

impl Catalog {
    pub fn finalize(mut self) -> Self {
        self.prompt_index = self.prompts.iter().map(|p| p.path.clone()).collect();
        self.instruction_index = self.instructions.iter().map(|i| i.path.clone()).collect();
        self.chat_mode_index = self.chat_modes.iter().map(|c| c.path.clone()).collect();
        self.collection_index = self.collections.iter().map(|c| c.path.clone()).collect();
        self.collection_lookup = self
            .collections
            .iter()
            .map(|c| (c.path.clone(), c.clone()))
            .collect();

        let mut membership: HashMap<String, Vec<String>> = HashMap::new();
        for collection in &self.collections {
            for item in &collection.items {
                membership
                    .entry(item.path.clone())
                    .or_default()
                    .push(collection.id.clone());
            }
        }
        for ids in membership.values_mut() {
            ids.sort();
        }
        self.membership = membership;
        self
    }

    pub fn contains(&self, kind: AssetKind, path: &str) -> bool {
        match kind {
            AssetKind::Prompt => self.prompt_index.contains(path),
            AssetKind::Instruction => self.instruction_index.contains(path),
            AssetKind::ChatMode => self.chat_mode_index.contains(path),
            AssetKind::Collection => self.collection_index.contains(path),
        }
    }

    pub fn collection_by_id(&self, id: &str) -> Option<&Collection> {
        self.collections.iter().find(|c| c.id == id)
    }

    pub fn collection_by_path(&self, path: &str) -> Option<&Collection> {
        self.collection_lookup.get(path)
    }

    pub fn memberships(&self, asset_path: &str) -> &[String] {
        self.membership
            .get(asset_path)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
}
