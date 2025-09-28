# Design

## Architectural Overview

The standalone Rust binary is organized into the following layers:

1. **CLI (`main.rs`)** parses flags (`--repo`, `--tick`), installs tracing, and delegates to the runtime.
2. **Paths (`io::paths`)** normalizes the project root, `.github` asset directories, enablement file location, cache directory (`.awesome-copilot-tui/cache`), and backup folder.
3. **Enablement Persistence (`io::enablement`)** loads and saves the schema-validated enablement JSON using the embedded copy at `docs/schemas/enablement.schema.json`, defaulting all assets to disabled when no prior state exists.
4. **Upstream Sync (`io::upstream`)** downloads and extracts the latest `github/awesome-copilot@main` archive, applying throttling and emitting structured warnings when offline.
5. **Catalog Builder (`io::catalog`)** walks the cached archive, parses front matter / YAML to produce normalized `Catalog` entries and upstream SHA-256 checksums.
6. **Domain State (`domain` module)** merges catalog metadata with persisted enablement flags and local filesystem state to compute enabled status, freshness, and collection rollups.
7. **Runtime & UI (`app`, `ui`)** manages the event loop, keyboard shortcuts, enable/disable orchestration, reset/save operations, backup handling, and renders the `ratatui` interface.

Network access, filesystem mutation, and UI rendering remain isolated, enabling deterministic unit tests across parsers and domain logic.

## Data Model

```text
Catalog
  ├── prompts: Vec<Prompt>
  ├── instructions: Vec<Instruction>
  ├── chat_modes: Vec<ChatMode>
  └── collections: Vec<Collection>
Prompt / Instruction / ChatMode
  ├── path: String          # relative project path
  ├── slug: String          # filename stem
  ├── name: String          # title-cased display name
  ├── description: String   # from front matter
  ├── tags: Vec<String>
  ├── apply_to / mode / tools
  ├── sha256: String        # upstream checksum
Collection
  ├── path: String (collections/<id>.collection.yml)
  ├── id, name, description, tags
  ├── items: Vec<CollectionItem>
AssetView (derived)
  ├── kind: AssetKind
  ├── path, slug, name, description, metadata …
  ├── enabled: bool         # effective value after explicit/inherited/default resolution
  ├── explicit: Option<bool>
  ├── inherited: Option<bool>
  ├── local: LocalStatus    # Missing | Same | Diff | NA
  ├── collections: Vec<CollectionRef>
  ├── member_count: usize   # collections only
LocalStatus
  ├── Missing               # no local file present
  ├── Same                  # local file matches upstream snapshot
  ├── Diff                  # local file differs from upstream
  ├── NA                    # not applicable (collections)
```

`Catalog::finalize` builds lookup maps for collection membership and path lookups to support fast domain recomputation.

## Upstream Synchronization

- Archive URL: `https://codeload.github.com/github/awesome-copilot/zip/refs/heads/main`.
- Cache layout: `.awesome-copilot-tui/cache/<commit>/…` under the project root.
- Freshness window: reuse cache younger than 12 hours; otherwise fetch a new archive. Users can force-refresh via the reload action.
- Commit detection: prefer the `X-GitHub-Source-Sha` header; fallback to the archive directory name (`awesome-copilot-<sha>`).
- Failure handling: if download fails and a cache exists, continue with a warning; if no cache is available, abort startup with actionable messaging.
- Cleanup: retain the five most recent caches and prune older snapshots asynchronously after successful extraction.

## Catalog Construction

1. Enumerate cached files with glob filters:
   - `instructions/**/*.instructions.md`
   - `prompts/**/*.prompt.md`
   - `chatmodes/**/*.chatmode.md`
   - `collections/**/*.collection.yml`
2. Parse Markdown front matter using `serde_yaml` to extract `description`, `applyTo`, `tags`, `mode`, `tools`, etc. Missing fields fall back to defaults (title-cased slug for name, empty lists for tags).
3. Collections are deserialized from YAML and validated so every referenced asset path exists in the catalog; invalid entries yield warnings and are skipped.
4. Compute SHA-256 for every upstream file and store alongside the catalog entry for later comparison.
5. Accumulate warnings for malformed front matter, missing descriptions, unsupported collection item kinds, or other recoverable issues.

## Local Comparison & Domain State

- Derive `local_path = .github/<kind>/<relative>` for each catalog entry.
- Compute `LocalStatus` by hashing the upstream snapshot and local file (when present).
- Resolve the effective enabled value with the following precedence:
  1. Explicit entry in the enablement file (`true`/`false`).
  2. Inherited entry from the first collection that references the asset and has an explicit value.
  3. Default disabled state when neither explicit nor inherited values exist.
- Collections summarize member counts (`enabled_count`, `diff_count`) for display inside the detail pane.

## Runtime Actions

- **Toggle Enable/Disable (`Space`/`Enter`)**
  1. Determine the desired state by flipping the current effective value.
  2. Update the enablement map for the asset: store an explicit value when it differs from the inherited baseline, or remove the entry when it matches.
  3. When enabling, ensure parent directories exist, back up modified files to `.awesome-copilot-tui/backups/<UTC timestamp>/<relative path>`, then copy the upstream snapshot into `.github/...`.
  4. When disabling, back up modified files if needed, delete the local file, and prune empty directories up to the asset root.
  5. Recompute domain state, refresh `LocalStatus`, and mark the session dirty.

- **Collection Toggle** iterates member assets, applying the enable/disable routine while respecting per-item explicit overrides. Failures append to warnings without aborting the batch.

- **Reset (`X`, TBD)** removes all prompt, instruction, and chat mode files under `.github/`, clears enablement entries, recomputes domain state, and marks the session dirty.

- **Save (`Ctrl+S`)** serializes the enablement file (including timestamp/version), validates it against the embedded JSON schema, and writes atomically to disk.

- **Reload (`r`)** refreshes the upstream cache (respecting the freshness window) and recomputes local comparisons.

- **Search (`/`)** filters assets by case-insensitive substrings across name, path, slug, description, and tags.

## UI Composition

- Header displays repository path, selected tab, dirty flag, active filter, number of modified assets, and cache commit SHA.
- Table columns:
  - `State` (badge: `✔` up-to-date, `~` modified, `✖` missing, `?` unexpected).
  - `Status` (text label summarizing install state).
  - `Name`, `Path`, `Tags`.
- Detail pane adds checksum info, last backup path, collection memberships, and raw description text.
- Footer cycles between key bindings, warnings/errors, and active prompts (quit/reload confirmations).

## Error Handling & Backups

- Network, parsing, and filesystem operations return `anyhow::Result` enriched with context for troubleshooting.
- Warnings capture recoverable conditions (stale cache, malformed metadata, partial collection toggles) and are surfaced in the footer.
- Backups mirror the original directory structure and include the timestamp + checksum in the filename to simplify restoration.
- Install/remove routines are transactional: if writing fails, the partially written file is removed and the original restored from the latest backup.

## Testing Strategy

- **Unit Tests**
  - Front matter/YAML parsers and slug-to-name normalization.
  - SHA-256 comparison helpers and status classification logic.
  - Install/remove routines using `tempfile` directories, verifying backups when modifications exist.

- **Integration Tests**
  - Fixture zip archive representing a trimmed upstream snapshot for deterministic network-free tests.
  - End-to-end scenarios (install, reinstall, remove, collection bulk operations) executed in temporary project roots.
  - Offline fallback path (download fails → cached snapshot reused).

- **Manual QA**
  - Run the TUI against empty and partially populated projects.
  - Validate backups, reset behavior, and save/load persistence manually.

  ## Copilot Instructions Artifact

  - **Purpose**: Provide a single source of truth (`.github/copilot/copilot-instructions.md`) that guides GitHub Copilot to mirror the repository's actual technology stack, architecture, and coding conventions.
  - **Inputs**:
    - Existing guidance under `.github/instructions`, `.github/prompts`, and related docs.
    - Manifest and configuration files (`package.json`, `Cargo.toml`, `*.csproj`, etc.) for version detection.
    - Representative source files that illustrate naming, testing, and error-handling patterns.
  - **Process**:
    1. Enumerate technology versions by inspecting package managers/build manifests across languages in the repo.
    2. Summarize architectural boundaries from folder structure and domain modules (e.g., Rust TUI layers, scripts, documentation generators).
    3. Capture recurring code conventions (naming, logging, testing) by sampling recent files in each primary language.
    4. Synthesize findings into the blueprint-provided template, ensuring every directive references observed patterns rather than assumptions.
  - **Outputs**: Markdown instructions with sections for priority guidelines, version expectations, quality standards, testing approaches, and technology-specific guidance populated with concrete repo examples.
  - **Validation**: Peer review to ensure statements cite verifiable patterns; rerun whenever major architectural or dependency changes land.

## Open Questions & Backlog

- Cache eviction policy refinement (retain five snapshots today; consider user configuration later).
- Support for alternative branches or forks (future CLI flags).
- Merge-aware removals (diff view before deleting modified files) remains a backlog item.
