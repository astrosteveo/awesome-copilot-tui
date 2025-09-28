use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct RepoPaths {
    pub root: PathBuf,
    pub github_dir: PathBuf,
    pub instructions_dir: PathBuf,
    pub prompts_dir: PathBuf,
    pub chatmodes_dir: PathBuf,
    pub collections_dir: PathBuf,
    pub workspace_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub backups_dir: PathBuf,
    pub enablement: PathBuf,
}

impl RepoPaths {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        let github_dir = root.join(".github");
        let workspace_dir = root.join(".awesome-copilot-tui");
        let cache_dir = workspace_dir.join("cache");
        let backups_dir = workspace_dir.join("backups");
        let enablement = workspace_dir.join("enablement.json");
        Self {
            github_dir: github_dir.clone(),
            instructions_dir: github_dir.join("instructions"),
            prompts_dir: github_dir.join("prompts"),
            chatmodes_dir: github_dir.join("chatmodes"),
            collections_dir: github_dir.join("collections"),
            workspace_dir,
            cache_dir,
            backups_dir,
            enablement,
            root,
        }
    }

    pub fn ensure_project_structure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.github_dir)?;
        std::fs::create_dir_all(&self.instructions_dir)?;
        std::fs::create_dir_all(&self.prompts_dir)?;
        std::fs::create_dir_all(&self.chatmodes_dir)?;
        // Do not create collections directory under .github: collections are a logical grouping only.
        std::fs::create_dir_all(&self.cache_dir)?;
        std::fs::create_dir_all(&self.backups_dir)?;
        Ok(())
    }

    pub fn asset_root(&self, kind: crate::domain::model::AssetKind) -> &Path {
        match kind {
            crate::domain::model::AssetKind::Prompt => &self.prompts_dir,
            crate::domain::model::AssetKind::Instruction => &self.instructions_dir,
            crate::domain::model::AssetKind::ChatMode => &self.chatmodes_dir,
            crate::domain::model::AssetKind::Collection => &self.collections_dir,
        }
    }
}
