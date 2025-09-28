# Implementation Tasks

| ID | Task | Description | Dependencies | Status |
| --- | --- | --- | --- | --- |
| T1 | Upstream sync module | Download and extract the `github/awesome-copilot` archive into a versioned cache with freshness + pruning rules. | None | Complete |
| T2 | Catalog reconstruction | Parse cached instructions/prompts/chat modes/collections, extract front matter, and compute upstream checksums. | T1 | Complete |
| T3 | Local state scanner | Detect installed assets by hashing local files, classify local status, and create collection rollups. | T2 | Complete |
| T4 | Domain overhaul | Replace legacy enablement model with asset views and expose helper APIs for the UI. | T2, T3 | Complete |
| T5 | UI refresh | Update table/summary widgets to show enabled state, local status, backups, and warnings. | T4 | Complete |
| T6 | Install/remove executor | Implement copy/remove operations with backup support, directory creation/pruning, and runtime integration. | T4 | Complete |
| T7 | Collection bulk actions | Apply enable/disable executor to collection members with aggregated warnings/results. | T6 | Complete |
| T8 | Reload & cache controls | Wire runtime reload command to force cache refresh (respecting cooldown) and surface cache metadata in UI. | T1, T4 | Complete |
| T9 | Tests & fixtures | Maintain fixtures and unit tests for catalog parsing, toggle semantics, and collection overrides. | T2, T3, T6 | In Progress |
| T10 | Documentation updates | Refresh README/help text once new workflow lands, including backup behavior and cache location. | T5, T6 | Planned |
| T11 | Technical debt cleanup | Remove legacy enablement schema code, address lint warnings, and track backlog items (cache eviction tuning, diff view). | T4, T9 | Planned |
| T12 | Copilot guidance research | Inventory technology versions, architecture boundaries, and code conventions to feed the Copilot instructions blueprint. | None | Complete |
| T13 | Copilot instructions draft | Populate `.github/copilot/copilot-instructions.md` using blueprint structure and research findings. | T12 | Complete |
| T14 | Copilot instructions review | Validate the drafted guidance against the repository, capture follow-up actions, and schedule periodic updates. | T13 | Complete |
| T15 | Default-disabled model | Ensure assets default to disabled, adjust domain resolution, and reconcile with existing enablement files. | T4 | In Progress |
| T16 | Reset command | Provide a runtime action that clears enablement state and removes `.github` asset files. | T6 | Planned |
| T17 | Save persistence | Implement schema-validated save flow and UI wiring for the enablement file. | T3, T6 | Planned |
| T18 | Persistence regression tests | Add coverage for default-disabled load, save, and reset workflows. | T9, T15–T17 | Planned |
| T19 | Standalone packaging | Move schema/docs into the crate and verify build without monorepo dependencies. | T15–T17 | Complete |

## Backlog / Deferred

- Fuzzy search with ranking (replace simple substring filter).
- External file change auto-detection with reload prompt.
- Theming and high-contrast mode options.
- Profiles and batch toggle operations.
