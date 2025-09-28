use crate::domain::{
    model::AssetKind,
    state::{AssetView, InheritedState},
};
use crate::io::sync::LocalStatus;

pub struct UiState;

pub fn state_badge(asset: &AssetView) -> String {
    if asset.effective {
        if asset.explicit == Some(true) {
            "✓ On".to_string()
        } else if asset.inherited.is_some() {
            "↳ On".to_string()
        } else {
            "• On".to_string()
        }
    } else {
        if asset.explicit == Some(false) {
            "✗ Off".to_string()
        } else if asset.inherited.is_some() {
            "↳ Off".to_string()
        } else {
            "• Off".to_string()
        }
    }
}

pub fn source_label(asset: &AssetView) -> String {
    if let Some(explicit) = asset.explicit {
        if explicit {
            "explicit:on".to_string()
        } else {
            "explicit:off".to_string()
        }
    } else if let Some(InheritedState { collection, value }) = &asset.inherited {
        format!("{}:{}", collection.id, if *value { "on" } else { "off" })
    } else {
        "default".to_string()
    }
}

pub fn tags_field(asset: &AssetView) -> String {
    match asset.kind {
        AssetKind::Instruction => {
            if !asset.apply_to.is_empty() {
                asset.apply_to.join(" | ")
            } else if !asset.tags.is_empty() {
                asset.tags.join(", ")
            } else {
                String::new()
            }
        }
        AssetKind::Collection => format!("{} items", asset.member_count),
        _ => {
            if !asset.tags.is_empty() {
                asset.tags.join(", ")
            } else {
                String::new()
            }
        }
    }
}

pub fn status_line(asset: &AssetView) -> String {
    let mut parts = Vec::new();
    parts.push(format!(
        "Effective: {}",
        if asset.effective { "on" } else { "off" }
    ));
    if let Some(explicit) = asset.explicit {
        parts.push(format!("Explicit: {}", explicit));
    }
    if let Some(inherited) = &asset.inherited {
        parts.push(format!(
            "Inherited: {} from {}",
            inherited.value, inherited.collection.id
        ));
    }
    parts.join(" | ")
}

pub fn collections_list(asset: &AssetView) -> String {
    if asset.collections.is_empty() {
        return "(none)".into();
    }
    asset
        .collections
        .iter()
        .map(|c| format!("{}", c.id))
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn toggle_preview(asset: &AssetView) -> String {
    let current_state = if asset.effective { "enabled" } else { "disabled" };
    let new_state = if asset.effective { "disabled" } else { "enabled" };
    
    let source = if asset.explicit.is_some() {
        "explicit"
    } else if asset.inherited.is_some() {
        "inherited"
    } else {
        "default"
    };
    
    if asset.kind == crate::domain::model::AssetKind::Collection {
        format!(
            "Currently: {} ({} state)\nToggle will: {} this collection and affect {} members",
            current_state, source, new_state, asset.member_count
        )
    } else {
        format!(
            "Currently: {} ({} state)\nToggle will: {} this asset",
            current_state, source, new_state
        )
    }
}

pub fn collection_toggle_impact(asset: &AssetView, domain_state: &crate::domain::state::DomainState) -> Option<String> {
    if asset.kind != crate::domain::model::AssetKind::Collection {
        return None;
    }
    
    // Find the collection to analyze its members
    let collection = domain_state.catalog.collections.iter()
        .find(|c| c.path == asset.path)?;
    
    let will_enable = !asset.effective;
    let mut impact_lines = Vec::new();
    let mut enable_count = 0;
    let mut disable_count = 0;
    let mut unchanged_count = 0;
    
    // Show up to 5 members with their state changes
    for item in collection.items.iter().take(5) {
        let member_assets = domain_state.assets(item.kind);
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
            
            let change_desc = if current_effective == new_effective {
                unchanged_count += 1;
                "no change"
            } else if new_effective {
                enable_count += 1;
                "will enable"
            } else {
                disable_count += 1;
                "will disable"
            };
            
            impact_lines.push(format!("  • {} ({})", member.name, change_desc));
        }
    }
    
    let total_members = collection.items.len();
    let mut summary = format!("Impact on {} member{}:", 
        total_members, if total_members == 1 { "" } else { "s" });
    
    if total_members > 5 {
        summary.push_str(&format!(" (showing first 5)"));
    }
    
    summary.push('\n');
    summary.push_str(&impact_lines.join("\n"));
    
    if enable_count > 0 || disable_count > 0 || unchanged_count > 0 {
        summary.push_str(&format!("\nSummary: {} enable, {} disable, {} unchanged", 
            enable_count, disable_count, unchanged_count));
    }
    
    Some(summary)
}

pub fn local_status(asset: &AssetView) -> String {
    match asset.local {
        LocalStatus::Missing => "Missing".into(),
        LocalStatus::Same => "Same".into(),
        LocalStatus::Diff => "Diff".into(),
        LocalStatus::NA => "N/A".into(),
    }
}
