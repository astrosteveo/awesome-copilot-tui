use anyhow::{anyhow, Result};

use super::{
    model::AssetKind,
    state::{AssetView, DomainState},
};

#[derive(Debug, Clone)]
pub struct ToggleResult {
    pub asset: AssetView,
}

pub fn toggle_asset(state: &mut DomainState, kind: AssetKind, path: &str) -> Result<ToggleResult> {
    let (current_effective, inherited_value) = {
        let assets = state.assets(kind);
        let asset = assets
            .iter()
            .find(|a| a.path == path)
            .ok_or_else(|| anyhow!("Asset not found for toggle: {}", path))?;
        (
            asset.effective,
            asset.inherited.as_ref().map(|inherit| inherit.value),
        )
    };

    let desired = !current_effective;
    let baseline = inherited_value.unwrap_or(true);

    let new_explicit = if desired == baseline {
        None
    } else {
        Some(desired)
    };

    let map = state.enablement.map_for_mut(kind);
    if let Some(value) = new_explicit {
        map.insert(path.to_string(), value);
    } else {
        map.remove(path);
    }

    state.recompute();

    let updated_asset = state
        .assets(kind)
        .iter()
        .find(|a| a.path == path)
        .cloned()
        .ok_or_else(|| anyhow!("Asset missing after toggle recompute: {}", path))?;

    Ok(ToggleResult {
        asset: updated_asset,
    })
}

/// Analyze the impact of toggling a collection to help with user confirmation
pub fn analyze_collection_toggle_impact(state: &DomainState, collection_path: &str) -> Result<CollectionToggleImpact> {
    let collection = state.catalog.collections.iter()
        .find(|c| c.path == collection_path)
        .ok_or_else(|| anyhow!("Collection not found: {}", collection_path))?;
    
    let collection_assets = state.assets(AssetKind::Collection);
    let collection_view = collection_assets.iter()
        .find(|a| a.path == collection_path)
        .ok_or_else(|| anyhow!("Collection view not found: {}", collection_path))?;
    
    let will_enable = !collection_view.effective;
    let mut enable_count = 0;
    let mut disable_count = 0;
    let mut unchanged_count = 0;
    let mut affected_members = Vec::new();
    
    for item in &collection.items {
        let member_assets = state.assets(item.kind);
        if let Some(member) = member_assets.iter().find(|a| a.path == item.path) {
            let current_effective = member.effective;
            let member_explicit = member.explicit;
            
            // Determine what the member's new effective state would be
            let new_effective = if member_explicit.is_some() {
                // If member has explicit setting, it won't change
                current_effective
            } else {
                // Member will inherit the new collection state
                will_enable
            };
            
            let impact = if current_effective == new_effective {
                unchanged_count += 1;
                MemberToggleImpact::Unchanged
            } else if new_effective {
                enable_count += 1;
                MemberToggleImpact::WillEnable
            } else {
                disable_count += 1;
                MemberToggleImpact::WillDisable
            };
            
            affected_members.push(MemberImpact {
                path: item.path.clone(),
                name: member.name.clone(),
                kind: item.kind,
                current_effective,
                new_effective,
                impact,
            });
        }
    }
    
    Ok(CollectionToggleImpact {
        collection_name: collection.name.clone(),
        collection_will_enable: will_enable,
        total_members: collection.items.len(),
        enable_count,
        disable_count,
        unchanged_count,
        affected_members,
    })
}

#[derive(Debug, Clone)]
pub struct CollectionToggleImpact {
    pub collection_name: String,
    pub collection_will_enable: bool,
    pub total_members: usize,
    pub enable_count: usize,
    pub disable_count: usize,
    pub unchanged_count: usize,
    pub affected_members: Vec<MemberImpact>,
}

#[derive(Debug, Clone)]
pub struct MemberImpact {
    pub path: String,
    pub name: String,
    pub kind: AssetKind,
    pub current_effective: bool,
    pub new_effective: bool,
    pub impact: MemberToggleImpact,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MemberToggleImpact {
    WillEnable,
    WillDisable,
    Unchanged,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        model::{Catalog, Collection, CollectionItem, EnablementFile, Instruction, Prompt},
        state::DomainState,
    };

    fn multi_catalog() -> Catalog {
        let instruction = Instruction {
            path: "instructions/test.instruction.md".into(),
            slug: "test-instruction".into(),
            name: "Test Instruction".into(),
            description: "A test instruction".into(),
            apply_to: vec!["language:rust".into()],
            tags: vec!["test".into()],
            sha256: "test-sha256".into(),
        };

        let prompt = Prompt {
            path: "prompts/test.prompt.md".into(),
            slug: "test-prompt".into(),
            name: "Test Prompt".into(),
            description: "A test prompt".into(),
            mode: "chat".into(),
            tags: vec!["test".into()],
            sha256: "test-sha256".into(),
        };

        let collection = Collection {
            path: "collections/test.collection.md".into(),
            id: "test-collection".into(),
            slug: "test-collection".into(),
            name: "Test Collection".into(),
            description: "A test collection".into(),
            tags: vec!["test".into()],
            items: vec![
                CollectionItem {
                    path: instruction.path.clone(),
                    kind: AssetKind::Instruction,
                },
                CollectionItem {
                    path: prompt.path.clone(),
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
    fn analyze_collection_toggle_impact_enable() {
        let catalog = multi_catalog();
        let collection_path = catalog.collections[0].path.clone();
        let mut enablement = EnablementFile::default();
        // Start with collection disabled
        enablement.collections.insert(collection_path.clone(), false);
        let state = DomainState::new(catalog, enablement);

        let impact = analyze_collection_toggle_impact(&state, &collection_path)
            .expect("analyze impact succeeds");

        assert_eq!(impact.collection_name, "Test Collection");
        assert!(impact.collection_will_enable);
        assert_eq!(impact.total_members, 2);
        assert_eq!(impact.enable_count, 2);
        assert_eq!(impact.disable_count, 0);
        assert_eq!(impact.unchanged_count, 0);
        assert_eq!(impact.affected_members.len(), 2);
        
        for member in &impact.affected_members {
            assert!(!member.current_effective);
            assert!(member.new_effective);
            assert_eq!(member.impact, MemberToggleImpact::WillEnable);
        }
    }

    #[test]
    fn analyze_collection_toggle_impact_disable() {
        let catalog = multi_catalog();
        let collection_path = catalog.collections[0].path.clone();
        let state = DomainState::new(catalog, EnablementFile::default());

        let impact = analyze_collection_toggle_impact(&state, &collection_path)
            .expect("analyze impact succeeds");

        assert_eq!(impact.collection_name, "Test Collection");
        assert!(!impact.collection_will_enable);
        assert_eq!(impact.total_members, 2);
        assert_eq!(impact.enable_count, 0);
        assert_eq!(impact.disable_count, 2);
        assert_eq!(impact.unchanged_count, 0);
        assert_eq!(impact.affected_members.len(), 2);
        
        for member in &impact.affected_members {
            assert!(member.current_effective);
            assert!(!member.new_effective);
            assert_eq!(member.impact, MemberToggleImpact::WillDisable);
        }
    }

    #[test]
    fn analyze_collection_toggle_impact_with_explicit_members() {
        let catalog = multi_catalog();
        let collection_path = catalog.collections[0].path.clone();
        let instruction_path = catalog.instructions[0].path.clone();
        let mut enablement = EnablementFile::default();
        
        // Collection is enabled by default, but explicitly disable the instruction
        enablement.instructions.insert(instruction_path, false);
        let state = DomainState::new(catalog, enablement);

        let impact = analyze_collection_toggle_impact(&state, &collection_path)
            .expect("analyze impact succeeds");

        assert!(!impact.collection_will_enable);
        assert_eq!(impact.total_members, 2);
        assert_eq!(impact.enable_count, 0);
        assert_eq!(impact.disable_count, 1); // Only the prompt will be disabled
        assert_eq!(impact.unchanged_count, 1); // The instruction remains explicitly disabled
        
        let instruction_impact = impact.affected_members.iter()
            .find(|m| m.kind == AssetKind::Instruction).unwrap();
        assert!(!instruction_impact.current_effective);
        assert!(!instruction_impact.new_effective);
        assert_eq!(instruction_impact.impact, MemberToggleImpact::Unchanged);
        
        let prompt_impact = impact.affected_members.iter()
            .find(|m| m.kind == AssetKind::Prompt).unwrap();
        assert!(prompt_impact.current_effective);
        assert!(!prompt_impact.new_effective);
        assert_eq!(prompt_impact.impact, MemberToggleImpact::WillDisable);
    }
}
