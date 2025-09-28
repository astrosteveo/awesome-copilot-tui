# Requirements

## Context

- Purpose: Provide a terminal user interface for selecting Awesome Copilot assets (instructions, prompts, chat modes, collections) and managing their presence inside any project repository.
- Inputs: Upstream archive of `github/awesome-copilot`, local project filesystem state, optional existing cached archive.
- Outputs: Project-local copies of selected assets, timestamped backups for overwritten or removed files, and status reporting within the TUI (no separate manifest required).
- Stakeholders: Repository maintainers, contributors who need to curate instruction sets quickly, downstream tooling consuming enablement state.

## Assumptions

- Repository root is the current working directory unless `--repo` overrides it.
- Users may run behind corporate proxies or without Git installed; the tool must rely on HTTPS downloads only.
- Upstream directories (`instructions/`, `prompts/`, `chatmodes/`, `collections/`) remain flat; nested folders are rare but must be handled gracefully.
- Local project layout mirrors upstream relative paths.

## Functional Requirements

- **RQ-1 (Startup preparation)**: WHEN the TUI starts, THE SYSTEM SHALL ensure the project contains `.github/instructions/`, `.github/prompts/`, and `.github/chatmodes/` directories, creating them when absent; collections remain logical-only.
- **RQ-2 (Enablement load)**: WHEN the TUI starts, THE SYSTEM SHALL load the persisted enablement file (if present) and default every asset to the disabled state when no explicit entry exists.
- **RQ-3 (Upstream sync)**: WHEN the TUI starts, THE SYSTEM SHALL download the latest archive of `github/awesome-copilot@main` into a local cache and extract it for catalog processing; IF the download fails but a prior cache exists, THEN THE SYSTEM SHALL reuse the cache and surface an offline warning; IF neither succeeds, THEN THE SYSTEM SHALL abort startup with a clear error.
- **RQ-4 (Catalog construction)**: WHEN upstream content is available, THE SYSTEM SHALL build catalog entries by parsing the upstream files' front matter (or sensible defaults) to populate descriptions, tags, and metadata for prompts, instructions, chat modes, and collections.
- **RQ-5 (Local comparison)**: WHEN catalog entries are built, THE SYSTEM SHALL compare each asset against the project directories to determine whether it is installed and whether the local file matches the cached upstream content.
- **RQ-6 (UI presentation)**: WHEN the user views any asset tab, THE SYSTEM SHALL display an "Enabled/Disabled" state, installation freshness (up-to-date vs. modified/outdated), and the file path for every asset alongside its metadata.
- **RQ-7 (Toggle enable)**: WHEN the user marks an asset as enabled, THE SYSTEM SHALL copy the upstream file from the cache into the project directory, overwriting the existing file only after creating a timestamped backup when local modifications differ from upstream, and persist the enabled flag in memory.
- **RQ-8 (Toggle disable)**: WHEN the user marks an asset as disabled, THE SYSTEM SHALL remove the project copy after creating a timestamped backup if local modifications differ from the cached upstream content, delete any corresponding enablement entry, and ensure the asset is absent from the project directory.
- **RQ-9 (Collection toggles)**: WHEN the user toggles a collection, THE SYSTEM SHALL apply the requested enable/disable action to every member asset using the same rules as individual toggles and report any failures as warnings.
- **RQ-10 (Reset action)**: WHEN the user invokes the reset action, THE SYSTEM SHALL remove all prompt, instruction, and chat mode files under `.github/` and clear all enablement entries.
- **RQ-11 (Save)**: WHEN the user requests a save, THE SYSTEM SHALL persist the current enablement file (including version and timestamp metadata) to disk using the embedded schema distributed with the tool, overwriting the previous file atomically.
- **RQ-12 (Refresh)**: WHEN the user requests a reload, THE SYSTEM SHALL re-download the upstream archive (subject to cache throttling) and recompute local comparison results without requiring a restart.
- **RQ-13 (Search)**: WHEN the user enters a search query, THE SYSTEM SHALL filter the visible list of assets using case-insensitive substring matching on name, path, slug, tags, and description, and allow clearing the filter with Escape.
- **RQ-14 (Copilot guidance)**: WHEN repository maintainers update developer guidance, THE SYSTEM SHALL provide a `.github/copilot/copilot-instructions.md` document that reflects observed technology versions, architectural patterns, and coding standards captured from the codebase without introducing unspecified practices.

## Non-Functional Requirements

- **NFR-1 (Responsiveness)**: THE SYSTEM SHALL process user input and update the UI within 100ms on a typical developer workstation.
- **NFR-2 (Reliability)**: THE SYSTEM SHALL guard against panics during normal operations and report actionable error messages.
- **NFR-3 (Usability)**: THE SYSTEM SHALL provide keyboard-first navigation with discoverable key bindings in the footer.
- **NFR-4 (Testability)**: THE SYSTEM SHALL expose pure functions for catalog parsing, enablement resolution, and explicit toggle logic to enable deterministic unit tests.

## Edge Cases & Failure Modes

- Upstream download failures, partial archives, or API throttling.
- Local filesystem write failures (permissions, read-only volumes, locked files).
- Asset content modified locally compared to upstream cache.
- Collections referencing assets that have been removed upstream.
- Assets removed from catalog but still present in enablement state.
- A single asset belonging to multiple collections with conflicting explicit toggles.
- Terminal resize during use.
- Concurrent external modification of `enablement.json` while the TUI is running.

## Dependencies & Constraints

- Rust stable toolchain (1.78 or above) with Cargo.
- `ratatui` and `crossterm` crates for terminal rendering and input.
- `jsonschema` crate (or equivalent) for schema validation.

## Confidence Assessment

- **Score**: 0.58
- **Rationale**: Enforcing a disabled-by-default model, bulk reset behavior, and schema-backed persistence introduces additional edge cases (state drift, partial failures, schema incompatibility). Requirements remain clear but the expanded surface area slightly lowers confidence until validation is complete.
