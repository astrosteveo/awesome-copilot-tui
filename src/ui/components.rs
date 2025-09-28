use crate::domain::{
    model::AssetKind,
    state::{AssetView, InheritedState},
};
use crate::io::sync::LocalStatus;

pub struct UiState;

pub fn state_badge(asset: &AssetView) -> String {
    let sign = if asset.effective { '+' } else { '-' };
    let flavor = if asset.explicit.is_some() {
        'E'
    } else if asset.inherited.is_some() {
        'I'
    } else {
        'D'
    };
    format!("{sign}{flavor}")
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

pub fn local_status(asset: &AssetView) -> String {
    match asset.local {
        LocalStatus::Missing => "Missing".into(),
        LocalStatus::Same => "Same".into(),
        LocalStatus::Diff => "Diff".into(),
        LocalStatus::NA => "N/A".into(),
    }
}
