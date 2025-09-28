use std::{
    collections::BTreeMap,
    io::{self, stdout},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::{
    domain::{model::AssetKind, state::DomainState},
    io::{
        catalog, enablement,
        paths::RepoPaths,
        sync::{self},
    },
    ui::draw,
};

#[derive(Debug, Parser)]
#[command(
    author,
    version,
    about = "Awesome Copilot asset enablement TUI (.github-managed)"
)]
struct Cli {
    /// Repository root (local assets live under .github/)
    #[arg(long, value_name = "PATH")]
    repo: Option<PathBuf>,

    /// UI tick rate in milliseconds for handling periodic events.
    #[arg(long = "tick", default_value_t = 250)]
    tick_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PendingPrompt {
    Quit,
    Reload,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SearchState {
    active: bool,
    query: String,
    draft: String,
}

impl SearchState {
    pub(crate) fn is_active(&self) -> bool {
        self.active
    }

    pub(crate) fn query(&self) -> &str {
        &self.query
    }

    pub(crate) fn draft(&self) -> &str {
        &self.draft
    }
}

pub struct App {
    paths: RepoPaths,
    upstream_dir: PathBuf,
    domain: DomainState,
    warnings: Vec<String>,
    message: Option<String>,
    error: Option<String>,
    dirty: bool,
    tab: AssetKind,
    selections: BTreeMap<AssetKind, usize>,
    search: SearchState,
    prompt: Option<PendingPrompt>,
    tick_rate: Duration,
    last_tick: Instant,
    should_quit: bool,
    shadow_current_assets: Option<Vec<crate::domain::state::AssetView>>, // filtered list with local statuses
}

pub fn run() -> Result<()> {
    install_tracing();
    let cli = Cli::parse();
    let repo = cli
        .repo
        .as_deref()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::current_dir().expect("working directory"));
    let paths = RepoPaths::new(repo);

    let catalog_load = catalog::load_catalog(&paths)?;
    let enablement_load = enablement::load_enablement(&paths)?;
    let mut warnings = catalog_load.warnings;
    warnings.extend(
        enablement_load
            .warnings
            .into_iter()
            .map(|warning| warning.to_string()),
    );
    let domain = DomainState::new(catalog_load.catalog, enablement_load.file);

    let mut app = App::new(
        paths,
        catalog_load.upstream_dir,
        domain,
        warnings,
        Duration::from_millis(cli.tick_ms),
    );
    app.run()?;
    Ok(())
}

impl App {
    fn new(
        paths: RepoPaths,
        upstream_dir: PathBuf,
        domain: DomainState,
        warnings: Vec<String>,
        tick_rate: Duration,
    ) -> Self {
        let mut selections = BTreeMap::new();
        selections.insert(AssetKind::Prompt, 0);
        selections.insert(AssetKind::Instruction, 0);
        selections.insert(AssetKind::ChatMode, 0);
        selections.insert(AssetKind::Collection, 0);
        Self {
            paths,
            upstream_dir,
            domain,
            warnings,
            message: None,
            error: None,
            dirty: false,
            tab: AssetKind::Instruction,
            selections,
            search: SearchState::default(),
            prompt: None,
            tick_rate,
            last_tick: Instant::now(),
            should_quit: false,
            shadow_current_assets: None,
        }
    }

    fn run(&mut self) -> Result<()> {
        enable_raw_mode().context("Failed to enable raw mode")?;
        let mut stdout = stdout();
        execute!(stdout, EnterAlternateScreen).context("Failed to enter alternate screen")?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend).context("Failed to initialize TUI terminal")?;
        terminal.clear()?;

        let res = self.event_loop(&mut terminal);

        disable_raw_mode().context("Failed to disable raw mode")?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)
            .context("Failed to leave alternate screen")?;
        terminal.show_cursor()?;

        res
    }

    fn event_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        loop {
            self.ensure_selection_bounds();
            self.compute_local_statuses()?;
            terminal.draw(|frame| draw::render(frame, self))?;

            if self.should_quit() {
                break;
            }

            let timeout = self
                .tick_rate
                .checked_sub(self.last_tick.elapsed())
                .unwrap_or(Duration::from_secs(0));

            if event::poll(timeout)? {
                match event::read()? {
                    Event::Key(key) => self.handle_key(key)?,
                    Event::Resize(_, _) => {
                        // redraw on next loop iteration
                    }
                    _ => {}
                }
            }

            if self.last_tick.elapsed() >= self.tick_rate {
                self.last_tick = Instant::now();
            }
        }

        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        if self.search.active {
            self.handle_search_key(key);
            return Ok(());
        }

        if let Some(prompt) = self.prompt {
            self.handle_prompt_key(prompt, key)?;
            return Ok(());
        }

        match key {
            KeyEvent {
                code: KeyCode::Char('q'),
                modifiers: KeyModifiers::NONE,
                ..
            } => self.request_quit(),
            KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::NONE,
                ..
            } => self.cleanup_orphans(),
            KeyEvent {
                code: KeyCode::Char('r'),
                modifiers: KeyModifiers::NONE,
                ..
            } => self.request_reload(),
            KeyEvent {
                code: KeyCode::Char('a'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                if let Err(err) = self.apply_selected() {
                    self.error = Some(format!("Apply failed: {err}"));
                } else {
                    self.message = Some("Applied from upstream".into());
                }
            }
            KeyEvent {
                code: KeyCode::Char('/'),
                modifiers: KeyModifiers::NONE,
                ..
            } => self.activate_search(),
            KeyEvent {
                code: KeyCode::Char('s'),
                modifiers,
                ..
            } if modifiers.contains(KeyModifiers::CONTROL) => {
                if let Err(err) = self.save() {
                    self.error = Some(format!("Save failed: {err}"));
                }
            }
            KeyEvent {
                code: KeyCode::Char('x'),
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                if let Err(err) = self.reset_assets() {
                    self.error = Some(format!("Reset failed: {err}"));
                }
            }
            KeyEvent {
                code: KeyCode::Tab,
                modifiers: KeyModifiers::NONE,
                ..
            } => self.next_tab(),
            KeyEvent {
                code: KeyCode::BackTab,
                ..
            } => self.prev_tab(),
            KeyEvent {
                code: KeyCode::Down,
                ..
            } => self.move_selection(1),
            KeyEvent {
                code: KeyCode::Up, ..
            } => self.move_selection(-1),
            KeyEvent {
                code: KeyCode::PageDown,
                ..
            } => self.move_selection(10),
            KeyEvent {
                code: KeyCode::PageUp,
                ..
            } => self.move_selection(-10),
            KeyEvent {
                code: KeyCode::Home,
                ..
            } => self.select_index(0),
            KeyEvent {
                code: KeyCode::End, ..
            } => self.select_last(),
            KeyEvent {
                code: KeyCode::Enter,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char(' '),
                ..
            } => {
                if let Err(err) = self.toggle_selection() {
                    self.error = Some(format!("Toggle failed: {err}"));
                }
            }
            KeyEvent {
                code: KeyCode::Esc, ..
            } => {
                self.clear_filter();
                self.message = None;
                self.error = None;
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.search.active = false;
                self.search.draft.clear();
            }
            KeyCode::Enter => {
                self.search.query = self.search.draft.trim().to_string();
                self.search.active = false;
                self.normalize_selection_after_filter();
            }
            KeyCode::Backspace => {
                self.search.draft.pop();
            }
            KeyCode::Char(ch) => {
                if !key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.search.draft.push(ch);
                }
            }
            KeyCode::Left => {
                // ignore for now
            }
            KeyCode::Right => {
                // ignore for now
            }
            _ => {}
        }
    }

    fn handle_prompt_key(&mut self, prompt: PendingPrompt, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('n') => {
                self.prompt = None;
                self.message = Some("Cancelled".to_string());
            }
            KeyCode::Char('y') | KeyCode::Enter => {
                self.prompt = None;
                match prompt {
                    PendingPrompt::Quit => {
                        self.set_quit();
                    }
                    PendingPrompt::Reload => {
                        self.reload()?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn request_quit(&mut self) {
        if self.dirty {
            self.prompt = Some(PendingPrompt::Quit);
            self.message =
                Some("Unsaved changes. Confirm quit with 'y' or cancel with Esc.".into());
        } else {
            self.set_quit();
        }
    }

    fn request_reload(&mut self) {
        if self.dirty {
            self.prompt = Some(PendingPrompt::Reload);
            self.message =
                Some("Unsaved changes. Reload and discard with 'y' or cancel with Esc.".into());
        } else if let Err(err) = self.reload() {
            self.error = Some(format!("Reload failed: {err}"));
        }
    }

    fn activate_search(&mut self) {
        self.search.active = true;
        self.search.draft = self.search.query.clone();
        self.message = Some("Search: type to filter, Enter to apply, Esc to cancel".into());
    }

    fn clear_filter(&mut self) {
        if !self.search.query.is_empty() {
            self.search.query.clear();
            self.normalize_selection_after_filter();
        }
    }

    fn move_selection(&mut self, delta: i32) {
        let len = self.filtered_assets(self.tab).len();
        if len == 0 {
            self.selections.insert(self.tab, 0);
            return;
        }
        let current = self.current_selection();
        let new_index = if delta.is_negative() {
            current.saturating_sub(delta.unsigned_abs() as usize)
        } else {
            (current + delta as usize).min(len.saturating_sub(1))
        };
        self.selections.insert(self.tab, new_index);
    }

    fn select_index(&mut self, index: usize) {
        let len = self.filtered_assets(self.tab).len();
        if len == 0 {
            self.selections.insert(self.tab, 0);
        } else {
            self.selections.insert(self.tab, index.min(len - 1));
        }
    }

    fn select_last(&mut self) {
        let len = self.filtered_assets(self.tab).len();
        if len == 0 {
            self.selections.insert(self.tab, 0);
        } else {
            self.selections.insert(self.tab, len - 1);
        }
    }

    fn toggle_selection(&mut self) -> Result<()> {
        if let Some(asset) = self.selected_asset().cloned() {
            let result =
                crate::domain::toggle::toggle_asset(&mut self.domain, asset.kind, &asset.path)?;
            // After state toggle, apply/remove local files accordingly.
            self.apply_after_toggle(asset.kind, &asset.path, &result)?;
            self.dirty = true;
            self.message = Some(format!(
                "{} → {} ({} state)",
                asset.path,
                if result.asset.effective {
                    "enabled"
                } else {
                    "disabled"
                },
                if result.asset.explicit.is_some() {
                    "explicit"
                } else if result.asset.inherited.is_some() {
                    "inherited"
                } else {
                    "default"
                }
            ));
            self.error = None;
            self.normalize_selection_after_filter();
        }
        Ok(())
    }

    fn apply_after_toggle(
        &mut self,
        kind: AssetKind,
        path: &str,
        result: &crate::domain::toggle::ToggleResult,
    ) -> Result<()> {
        match kind {
            AssetKind::Collection => {
                // For collections, iterate member assets and sync each according to new effective state
                if let Some(collection) = self.domain.catalog.collection_by_path(path) {
                    for item in &collection.items {
                        // Find asset view for the item to know its effective state after toggle
                        let views = self.domain.assets(item.kind);
                        if let Some(view) = views.iter().find(|v| v.path == item.path) {
                            if view.effective {
                                // Ensure applied
                                sync::apply_from_upstream(
                                    &self.paths,
                                    &self.upstream_dir,
                                    item.kind,
                                    &item.path,
                                )?;
                            } else {
                                // Remove if exists
                                let _ = sync::remove_local(&self.paths, item.kind, &item.path)?;
                            }
                        }
                    }
                    // Refresh local statuses for current list
                    self.compute_local_statuses()?;
                }
            }
            AssetKind::Prompt | AssetKind::Instruction | AssetKind::ChatMode => {
                if result.asset.effective {
                    sync::apply_from_upstream(&self.paths, &self.upstream_dir, kind, path)?;
                } else {
                    let _ = sync::remove_local(&self.paths, kind, path)?;
                }
                self.compute_local_statuses()?;
            }
        }
        Ok(())
    }

    fn reset_assets(&mut self) -> Result<()> {
        use crate::domain::model::AssetKind::{ChatMode, Instruction, Prompt};

        let kinds = [Prompt, Instruction, ChatMode];
        for kind in kinds {
            let paths: Vec<String> = self
                .domain
                .assets(kind)
                .iter()
                .map(|asset| asset.path.clone())
                .collect();
            for asset_path in paths {
                let _ = sync::remove_local(&self.paths, kind, &asset_path)?;
            }
        }

        self.domain.enablement.prompts.clear();
        self.domain.enablement.instructions.clear();
        self.domain.enablement.chat_modes.clear();
        self.domain.enablement.collections.clear();
        self.domain.enablement.overrides.clear();
        self.domain.enablement.updated_at = None;

        self.domain.recompute();
        self.shadow_current_assets = None;
        self.compute_local_statuses()?;

        self.dirty = true;
        self.message = Some("Cleared local assets and enablement state".into());
        self.error = None;
        Ok(())
    }

    fn cleanup_orphans(&mut self) {
        let removed = self.domain.cleanup_orphans();
        if removed > 0 {
            self.dirty = true;
            self.message = Some(format!("Removed {removed} orphan enablement entries"));
        } else {
            self.message = Some("No orphan entries to clean".into());
        }
        self.error = None;
    }

    fn save(&mut self) -> Result<()> {
        enablement::save_enablement(&self.paths, &mut self.domain.enablement)
            .context("failed to write enablement file")?;
        self.dirty = false;
        self.message = Some("Enablement saved".to_string());
        self.error = None;
        Ok(())
    }

    fn reload(&mut self) -> Result<()> {
        let catalog_load = catalog::load_catalog(&self.paths)?;
        let enablement_load = enablement::load_enablement(&self.paths)?;
        self.warnings = catalog_load.warnings;
        self.warnings.extend(
            enablement_load
                .warnings
                .into_iter()
                .map(|warning| warning.to_string()),
        );
        self.domain = DomainState::new(catalog_load.catalog, enablement_load.file);
        self.upstream_dir = catalog_load.upstream_dir;
        self.dirty = false;
        self.prompt = None;
        self.message = Some("Reloaded from disk".into());
        self.error = None;
        self.shadow_current_assets = None;
        self.compute_local_statuses()?;
        Ok(())
    }

    fn compute_local_statuses(&mut self) -> Result<()> {
        // Update local status for current filtered list to keep it cheap and consistent with UI.
        let upstream = self.upstream_dir.clone();
        let filtered = self.filtered_assets(self.tab);
        let mut shadow = Vec::with_capacity(filtered.len());
        for view in filtered.into_iter().cloned() {
            let status = sync::compute_local_status(&self.paths, &upstream, view.kind, &view.path)?;
            let mut v = view;
            v.local = status;
            shadow.push(v);
        }
        self.shadow_current_assets = Some(shadow);
        Ok(())
    }

    fn apply_selected(&mut self) -> Result<()> {
        if let Some(asset) = self.selected_asset().cloned() {
            if asset.kind == AssetKind::Collection {
                // No direct apply for collections
                self.message = Some("Collections have no files to apply".into());
                return Ok(());
            }
            sync::apply_from_upstream(&self.paths, &self.upstream_dir, asset.kind, &asset.path)?;
            // Recompute local statuses to reflect updated file
            self.compute_local_statuses()?;
        }
        Ok(())
    }

    fn next_tab(&mut self) {
        self.tab = match self.tab {
            AssetKind::Prompt => AssetKind::Instruction,
            AssetKind::Instruction => AssetKind::ChatMode,
            AssetKind::ChatMode => AssetKind::Collection,
            AssetKind::Collection => AssetKind::Prompt,
        };
        self.normalize_selection_after_filter();
    }

    fn prev_tab(&mut self) {
        self.tab = match self.tab {
            AssetKind::Prompt => AssetKind::Collection,
            AssetKind::Instruction => AssetKind::Prompt,
            AssetKind::ChatMode => AssetKind::Instruction,
            AssetKind::Collection => AssetKind::ChatMode,
        };
        self.normalize_selection_after_filter();
    }

    fn filtered_assets(&self, kind: AssetKind) -> Vec<&crate::domain::state::AssetView> {
        let assets = self.domain.assets(kind);
        if self.search.query.is_empty() {
            return assets.iter().collect();
        }
        let query = self.search.query.to_lowercase();
        assets
            .iter()
            .filter(|asset| asset_matches(asset, &query))
            .collect()
    }

    fn selected_asset(&self) -> Option<&crate::domain::state::AssetView> {
        let filtered = self.filtered_assets(self.tab);
        if filtered.is_empty() {
            None
        } else {
            let idx = self.current_selection().min(filtered.len() - 1);
            Some(filtered[idx])
        }
    }

    fn current_selection(&self) -> usize {
        self.selections.get(&self.tab).copied().unwrap_or(0)
    }

    fn ensure_selection_bounds(&mut self) {
        self.normalize_selection_after_filter();
    }

    fn normalize_selection_after_filter(&mut self) {
        let len = self.filtered_assets(self.tab).len();
        let entry = self.selections.entry(self.tab).or_insert(0);
        if len == 0 {
            *entry = 0;
        } else if *entry >= len {
            *entry = len - 1;
        }
    }

    fn set_quit(&mut self) {
        self.prompt = None;
        self.message = Some("Goodbye".into());
        self.should_quit = true;
    }

    fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn repo_root(&self) -> &Path {
        &self.paths.root
    }

    pub fn tab(&self) -> AssetKind {
        self.tab
    }

    pub fn search_state(&self) -> &SearchState {
        &self.search
    }

    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    pub fn message(&self) -> Option<&str> {
        self.error.as_deref().or(self.message.as_deref())
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn info_message(&self) -> Option<&str> {
        if self.error.is_some() {
            None
        } else {
            self.message.as_deref()
        }
    }

    pub fn dirty(&self) -> bool {
        self.dirty
    }

    pub fn prompt(&self) -> Option<PendingPrompt> {
        self.prompt
    }

    pub fn current_assets(&self) -> Vec<&crate::domain::state::AssetView> {
        if let Some(shadow) = &self.shadow_current_assets {
            return shadow.iter().collect();
        }
        self.filtered_assets(self.tab)
    }

    pub fn selection_index(&self) -> Option<usize> {
        let assets = self.current_assets();
        if assets.is_empty() {
            None
        } else {
            Some(self.current_selection().min(assets.len() - 1))
        }
    }

    pub fn selected_asset_view(&self) -> Option<&crate::domain::state::AssetView> {
        if let Some(shadow) = &self.shadow_current_assets {
            if shadow.is_empty() {
                return None;
            }
            let idx = self.current_selection().min(shadow.len() - 1);
            return shadow.get(idx);
        }
        self.selected_asset()
    }

    pub fn orphan_count(&self) -> usize {
        self.domain.orphans().len()
    }
}

fn install_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .try_init();
}

// TODO: collect_warnings removed - warnings now come directly from catalog load

fn asset_matches(asset: &crate::domain::state::AssetView, query: &str) -> bool {
    let haystacks = [
        asset.name.as_str(),
        asset.path.as_str(),
        asset.slug.as_deref().unwrap_or(""),
        &asset.description,
    ];
    if haystacks.iter().any(|v| v.to_lowercase().contains(query)) {
        return true;
    }
    if asset
        .tags
        .iter()
        .any(|tag| tag.to_lowercase().contains(query))
    {
        return true;
    }
    if asset
        .apply_to
        .iter()
        .any(|item| item.to_lowercase().contains(query))
    {
        return true;
    }
    asset
        .collections
        .iter()
        .any(|c| c.id.to_lowercase().contains(query) || c.name.to_lowercase().contains(query))
}

// TODO: EnablementWarning Display implementation removed with new architecture

impl Default for App {
    fn default() -> Self {
        panic!("App::default should not be used")
    }
}
