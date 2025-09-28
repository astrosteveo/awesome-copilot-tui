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
