use std::collections::BTreeMap;

use super::model::{AssetKind, Catalog, ChatMode, Collection, EnablementFile, Instruction, Prompt};
use crate::io::sync::LocalStatus;

#[derive(Debug, Clone)]
pub struct CollectionRef {
    pub id: String,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct InheritedState {
    pub collection: CollectionRef,
    pub value: bool,
}

#[derive(Debug, Clone)]
pub struct AssetView {
    pub kind: AssetKind,
    pub path: String,
    pub slug: Option<String>,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub apply_to: Vec<String>,
    pub mode: Option<String>,
    pub tools: Vec<String>,
    pub collections: Vec<CollectionRef>,
    pub member_count: usize,
    pub explicit: Option<bool>,
    pub inherited: Option<InheritedState>,
    pub effective: bool,
    pub local: LocalStatus,
}

#[derive(Debug, Clone)]
pub struct OrphanEntry {
    pub kind: AssetKind,
    pub path: String,
    pub value: bool,
}

#[derive(Debug, Default, Clone)]
pub struct DomainState {
    pub catalog: Catalog,
    pub enablement: EnablementFile,
    assets: BTreeMap<AssetKind, Vec<AssetView>>,
    orphans: Vec<OrphanEntry>,
}

impl DomainState {
    pub fn new(catalog: Catalog, enablement: EnablementFile) -> Self {
        let catalog = catalog.finalize();
        let mut state = Self {
            catalog,
            enablement,
            assets: BTreeMap::new(),
            orphans: Vec::new(),
        };
        state.recompute();
        state
    }

    pub fn assets(&self, kind: AssetKind) -> &[AssetView] {
        self.assets.get(&kind).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn orphans(&self) -> &[OrphanEntry] {
        &self.orphans
    }

    pub fn recompute(&mut self) {
        self.assets.clear();

        let mut prompts: Vec<_> = self
            .catalog
            .prompts
            .iter()
            .map(|p| self.build_prompt_view(p))
            .collect();
        prompts.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        self.assets.insert(AssetKind::Prompt, prompts);

        let mut instructions: Vec<_> = self
            .catalog
            .instructions
            .iter()
            .map(|i| self.build_instruction_view(i))
            .collect();
        instructions.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        self.assets.insert(AssetKind::Instruction, instructions);

        let mut chat_modes: Vec<_> = self
            .catalog
            .chat_modes
            .iter()
            .map(|c| self.build_chat_mode_view(c))
            .collect();
        chat_modes.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        self.assets.insert(AssetKind::ChatMode, chat_modes);

        let mut collections: Vec<_> = self
            .catalog
            .collections
            .iter()
            .map(|c| self.build_collection_view(c))
            .collect();
        collections.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        self.assets.insert(AssetKind::Collection, collections);

        self.orphans = self.collect_orphans();
    }

    fn build_prompt_view(&self, prompt: &Prompt) -> AssetView {
        let explicit = self.explicit_state(AssetKind::Prompt, &prompt.path);
        let inherited = self.inherited_state(&prompt.path);
        let effective =
            explicit.unwrap_or_else(|| inherited.as_ref().map(|s| s.value).unwrap_or(false));
        AssetView {
            kind: AssetKind::Prompt,
            path: prompt.path.clone(),
            slug: Some(prompt.slug.clone()),
            name: prompt.name.clone(),
            description: prompt.description.clone(),
            tags: prompt.tags.clone(),
            apply_to: Vec::new(),
            mode: if prompt.mode.is_empty() {
                None
            } else {
                Some(prompt.mode.clone())
            },
            tools: Vec::new(),
            collections: self.collections_for(&prompt.path),
            member_count: 0,
            explicit,
            inherited,
            effective,
            local: LocalStatus::NA,
        }
    }

    fn build_instruction_view(&self, instruction: &Instruction) -> AssetView {
        let explicit = self.explicit_state(AssetKind::Instruction, &instruction.path);
        let inherited = self.inherited_state(&instruction.path);
        let effective =
            explicit.unwrap_or_else(|| inherited.as_ref().map(|s| s.value).unwrap_or(false));
        AssetView {
            kind: AssetKind::Instruction,
            path: instruction.path.clone(),
            slug: Some(instruction.slug.clone()),
            name: instruction.name.clone(),
            description: instruction.description.clone(),
            tags: instruction.tags.clone(),
            apply_to: instruction.apply_to.clone(),
            mode: None,
            tools: Vec::new(),
            collections: self.collections_for(&instruction.path),
            member_count: 0,
            explicit,
            inherited,
            effective,
            local: LocalStatus::NA,
        }
    }

    fn build_chat_mode_view(&self, mode: &ChatMode) -> AssetView {
        let explicit = self.explicit_state(AssetKind::ChatMode, &mode.path);
        let inherited = self.inherited_state(&mode.path);
        let effective =
            explicit.unwrap_or_else(|| inherited.as_ref().map(|s| s.value).unwrap_or(false));
        AssetView {
            kind: AssetKind::ChatMode,
            path: mode.path.clone(),
            slug: Some(mode.slug.clone()),
            name: mode.name.clone(),
            description: mode.description.clone(),
            tags: mode.tags.clone(),
            apply_to: Vec::new(),
            mode: None,
            tools: mode.tools.clone(),
            collections: self.collections_for(&mode.path),
            member_count: 0,
            explicit,
            inherited,
            effective,
            local: LocalStatus::NA,
        }
    }

    fn build_collection_view(&self, collection: &Collection) -> AssetView {
        let explicit = self.explicit_state(AssetKind::Collection, &collection.path);
        let effective = explicit.unwrap_or(false);
        AssetView {
            kind: AssetKind::Collection,
            path: collection.path.clone(),
            slug: Some(collection.id.clone()),
            name: collection.name.clone(),
            description: collection.description.clone(),
            tags: collection.tags.clone(),
            apply_to: Vec::new(),
            mode: None,
            tools: Vec::new(),
            collections: Vec::new(),
            member_count: collection.items.len(),
            explicit,
            inherited: None,
            effective,
            local: LocalStatus::NA,
        }
    }

    fn explicit_state(&self, kind: AssetKind, path: &str) -> Option<bool> {
        self.enablement.map_for(kind).get(path).copied()
    }

    fn inherited_state(&self, path: &str) -> Option<InheritedState> {
        let memberships = self.catalog.memberships(path);
        let mut candidates: Vec<(CollectionRef, bool)> = Vec::new();
        for collection_id in memberships {
            if let Some(collection) = self.catalog.collection_by_id(collection_id) {
                if let Some(value) = self
                    .enablement
                    .map_for(AssetKind::Collection)
                    .get(&collection.path)
                {
                    candidates.push((
                        CollectionRef {
                            id: collection.id.clone(),
                            name: collection.name.clone(),
                            path: collection.path.clone(),
                        },
                        *value,
                    ));
                }
            }
        }
        candidates.sort_by(|a, b| a.0.id.cmp(&b.0.id));
        candidates
            .into_iter()
            .next()
            .map(|(collection, value)| InheritedState { collection, value })
    }

    fn collections_for(&self, path: &str) -> Vec<CollectionRef> {
        self.catalog
            .memberships(path)
            .iter()
            .filter_map(|id| self.catalog.collection_by_id(id))
            .map(|c| CollectionRef {
                id: c.id.clone(),
                name: c.name.clone(),
                path: c.path.clone(),
            })
            .collect()
    }

    fn collect_orphans(&self) -> Vec<OrphanEntry> {
        let mut result = Vec::new();
        for (path, value) in &self.enablement.prompts {
            if !self.catalog.contains(AssetKind::Prompt, path) {
                result.push(OrphanEntry {
                    kind: AssetKind::Prompt,
                    path: path.clone(),
                    value: *value,
                });
            }
        }
        for (path, value) in &self.enablement.instructions {
            if !self.catalog.contains(AssetKind::Instruction, path) {
                result.push(OrphanEntry {
                    kind: AssetKind::Instruction,
                    path: path.clone(),
                    value: *value,
                });
            }
        }
        for (path, value) in &self.enablement.chat_modes {
            if !self.catalog.contains(AssetKind::ChatMode, path) {
                result.push(OrphanEntry {
                    kind: AssetKind::ChatMode,
                    path: path.clone(),
                    value: *value,
                });
            }
        }
        for (path, value) in &self.enablement.collections {
            if !self.catalog.contains(AssetKind::Collection, path) {
                result.push(OrphanEntry {
                    kind: AssetKind::Collection,
                    path: path.clone(),
                    value: *value,
                });
            }
        }
        result.sort_by(|a, b| a.path.cmp(&b.path));
        result
    }

    pub fn cleanup_orphans(&mut self) -> usize {
        let orphans = self.collect_orphans();
        let removed = orphans.len();
        if removed == 0 {
            return 0;
        }
        for orphan in &orphans {
            self.enablement.remove(orphan.kind, &orphan.path);
        }
        self.recompute();
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        model::{
            AssetKind, Catalog, Collection, CollectionItem, EnablementFile, Instruction, Prompt,
        },
        toggle,
    };

    fn sample_catalog() -> Catalog {
        let instruction_path = "instructions/sample.instructions.md".to_string();
        let collection_path = "collections/sample.collection.yml".to_string();
        let instruction = Instruction {
            path: instruction_path.clone(),
            slug: "sample".into(),
            name: "Sample Instruction".into(),
            description: "Helps with testing".into(),
            apply_to: vec!["**/*.rs".into()],
            tags: vec!["test".into()],
            sha256: "test-sha256".into(),
        };
        let collection = Collection {
            path: collection_path,
            id: "sample".into(),
            slug: "sample".into(),
            name: "Sample Collection".into(),
            description: "Bundle for testing".into(),
            tags: vec![],
            items: vec![CollectionItem {
                path: instruction_path,
                kind: AssetKind::Instruction,
            }],
            sha256: "test-sha256".into(),
        };

        Catalog {
            prompts: vec![],
            instructions: vec![instruction],
            chat_modes: vec![],
            collections: vec![collection],
            ..Catalog::default()
        }
        .finalize()
    }

    fn multi_catalog() -> Catalog {
        let instruction_path = "instructions/sample.instructions.md".to_string();
        let prompt_path = "prompts/sample.prompt.md".to_string();
        let collection_path = "collections/sample.collection.yml".to_string();

        let instruction = Instruction {
            path: instruction_path.clone(),
            slug: "sample".into(),
            name: "Sample Instruction".into(),
            description: "Helps with testing".into(),
            apply_to: vec!["**/*.rs".into()],
            tags: vec!["test".into()],
            sha256: "test-sha256".into(),
        };
        let prompt = Prompt {
            path: prompt_path.clone(),
            slug: "sample-prompt".into(),
            name: "Sample Prompt".into(),
            description: "Prompt for testing".into(),
            mode: String::new(),
            tags: vec!["test".into()],
            sha256: "test-sha256".into(),
        };
        let collection = Collection {
            path: collection_path,
            id: "bundle".into(),
            slug: "bundle".into(),
            name: "Bundle".into(),
            description: "Bundle for testing".into(),
            tags: vec![],
            items: vec![
                CollectionItem {
                    path: instruction_path,
                    kind: AssetKind::Instruction,
                },
                CollectionItem {
                    path: prompt_path,
                    kind: AssetKind::Prompt,
                },
            ],
            sha256: "test-sha256".into(),
        };

        Catalog {
            prompts: vec![prompt],
            instructions: vec![instruction],
            chat_modes: vec![],
            collections: vec![collection],
            ..Catalog::default()
        }
        .finalize()
    }

    #[test]
    fn toggle_collection_off_disables_members() {
        let catalog = multi_catalog();
        let collection_path = catalog.collections[0].path.clone();
        let mut enablement = EnablementFile::default();
        // Start with collection enabled
        enablement.collections.insert(collection_path.clone(), true);
        let mut state = DomainState::new(catalog, enablement);

        // Toggle the collection off (from enabled to disabled)
        let _ = toggle::toggle_asset(&mut state, AssetKind::Collection, &collection_path)
            .expect("toggle collection off succeeds");

        // Both instruction and prompt should inherit disabled state
        let inst = state.assets(AssetKind::Instruction).first().unwrap();
        assert!(!inst.effective);
        assert!(inst.explicit.is_none());
        assert!(inst.inherited.is_none()); // No inherited state since collection was removed from enablement

        let prm = state.assets(AssetKind::Prompt).first().unwrap();
        assert!(!prm.effective);
        assert!(prm.explicit.is_none());
        assert!(prm.inherited.is_none()); // No inherited state since collection was removed from enablement
    }

    #[test]
    fn toggle_collection_on_enables_members() {
        let catalog = multi_catalog();
        let collection_path = catalog.collections[0].path.clone();
        let mut enablement = EnablementFile::default();
        // Start with collection disabled
        enablement
            .collections
            .insert(collection_path.clone(), false);
        let mut state = DomainState::new(catalog, enablement);

        // Toggle the collection on
        let _ = toggle::toggle_asset(&mut state, AssetKind::Collection, &collection_path)
            .expect("toggle collection on succeeds");

        // Both instruction and prompt should now be enabled via inheritance from the collection
        let inst = state.assets(AssetKind::Instruction).first().unwrap();
        assert!(inst.effective);
        assert!(inst.explicit.is_none());
        assert_eq!(inst.inherited.as_ref().unwrap().value, true);

        let prm = state.assets(AssetKind::Prompt).first().unwrap();
        assert!(prm.effective);
        assert!(prm.explicit.is_none());
        assert_eq!(prm.inherited.as_ref().unwrap().value, true);
    }

    #[test]
    fn clean_project_assets_disabled_by_default() {
        let catalog = multi_catalog();
        let state = DomainState::new(catalog, EnablementFile::default());

        // In a clean project, all assets should be disabled by default
        let inst = state.assets(AssetKind::Instruction).first().unwrap();
        assert!(!inst.effective);
        assert!(inst.explicit.is_none());
        assert!(inst.inherited.is_none());

        let prm = state.assets(AssetKind::Prompt).first().unwrap();
        assert!(!prm.effective);
        assert!(prm.explicit.is_none());
        assert!(prm.inherited.is_none());

        let collection = state.assets(AssetKind::Collection).first().unwrap();
        assert!(!collection.effective);
        assert!(collection.explicit.is_none());
        assert!(collection.inherited.is_none());
    }

    #[test]
    fn explicit_collection_true_cascades_enabled() {
        let catalog = multi_catalog();
        let collection_path = catalog.collections[0].path.clone();
        let mut enablement = EnablementFile::default();
        enablement.collections.insert(collection_path, true);
        let state = DomainState::new(catalog, enablement);

        let inst = state.assets(AssetKind::Instruction).first().unwrap();
        assert!(inst.effective);
        assert!(inst.explicit.is_none());
        assert_eq!(inst.inherited.as_ref().unwrap().value, true);

        let prm = state.assets(AssetKind::Prompt).first().unwrap();
        assert!(prm.effective);
        assert!(prm.explicit.is_none());
        assert_eq!(prm.inherited.as_ref().unwrap().value, true);
    }

    #[test]
    fn instruction_inherits_collection_disable() {
        let catalog = sample_catalog();
        let collection_path = &catalog.collections[0].path;
        let mut enablement = EnablementFile::default();
        enablement
            .collections
            .insert(collection_path.clone(), false);
        let state = DomainState::new(catalog, enablement);
        let instruction = state
            .assets(AssetKind::Instruction)
            .first()
            .expect("instruction present");
        assert!(!instruction.effective);
        assert!(instruction.explicit.is_none());
        assert!(instruction.inherited.as_ref().is_some());
    }

    #[test]
    fn toggle_overrides_inherited_state() {
        let catalog = sample_catalog();
        let instruction_path = catalog.instructions[0].path.clone();
        let collection_path = catalog.collections[0].path.clone();
        let mut enablement = EnablementFile::default();
        enablement
            .collections
            .insert(collection_path.clone(), false);
        let mut state = DomainState::new(catalog, enablement);

        let result = toggle::toggle_asset(&mut state, AssetKind::Instruction, &instruction_path)
            .expect("toggle succeeds");
        assert!(result.asset.effective);
        assert_eq!(
            state
                .enablement
                .instructions
                .get(&instruction_path)
                .copied(),
            Some(true)
        );
        // Collection entry remains unchanged.
        assert_eq!(
            state.enablement.collections.get(&collection_path).copied(),
            Some(false)
        );
    }

    #[test]
    fn cleanup_removes_orphans() {
        let catalog = sample_catalog();
        let mut enablement = EnablementFile::default();
        enablement
            .prompts
            .insert("prompts/orphan.prompt.md".into(), true);
        let mut state = DomainState::new(catalog, enablement);
        assert_eq!(state.orphans().len(), 1);
        let removed = state.cleanup_orphans();
        assert_eq!(removed, 1);
        assert!(state.orphans().is_empty());
        assert!(state
            .enablement
            .prompts
            .get("prompts/orphan.prompt.md")
            .is_none());
    }

    #[test]
    fn collection_disable_preserves_explicit_true_on_item() {
        let catalog = sample_catalog();
        let instruction_path = catalog.instructions[0].path.clone();
        let collection_path = catalog.collections[0].path.clone();

        let mut enablement = EnablementFile::default();
        // Explicitly enable the instruction
        enablement
            .instructions
            .insert(instruction_path.clone(), true);
        // Disable the collection
        enablement.collections.insert(collection_path, false);

        let state = DomainState::new(catalog, enablement);
        let view = state
            .assets(AssetKind::Instruction)
            .iter()
            .find(|a| a.path == instruction_path)
            .expect("instruction present");
        // Effective remains true because explicit overrides inherited false
        assert!(view.effective);
        assert_eq!(view.explicit, Some(true));
        assert!(view.inherited.as_ref().is_some());
        assert_eq!(view.inherited.as_ref().unwrap().value, false);
    }

    #[test]
    fn collection_enable_preserves_explicit_false_on_item() {
        let catalog = sample_catalog();
        let instruction_path = catalog.instructions[0].path.clone();
        let collection_path = catalog.collections[0].path.clone();

        let mut enablement = EnablementFile::default();
        // Explicitly disable the instruction
        enablement
            .instructions
            .insert(instruction_path.clone(), false);
        // Enable the collection
        enablement.collections.insert(collection_path, true);

        let state = DomainState::new(catalog, enablement);
        let view = state
            .assets(AssetKind::Instruction)
            .iter()
            .find(|a| a.path == instruction_path)
            .expect("instruction present");
        // Effective remains false because explicit overrides inherited true
        assert!(!view.effective);
        assert_eq!(view.explicit, Some(false));
        assert!(view.inherited.as_ref().is_some());
        assert_eq!(view.inherited.as_ref().unwrap().value, true);
    }
}
