# Awesome Copilot TUI

A terminal user interface for viewing and managing the enablement state of Awesome Copilot assets (instructions, prompts, chat modes, and collections). The application loads `data/asset-metadata.json` and `data/enablement.json`, applies the cascading rules defined in `docs/enablement-workflow.md`, and lets you toggle assets with instant feedback.

## Prerequisites

- Rust 1.78 (stable) or newer
- Repository cloned locally with up-to-date `data/asset-metadata.json`

## Building & Running

Run the TUI from the repository root:

```bash
cargo run --package awesome-copilot-tui -- --repo .
```

You can point `--repo` at any clone of the repository. By default the current working directory is used.

### Key Bindings

| Keys | Action |
| --- | --- |
| `Tab` / `Shift+Tab` | Switch between asset kinds |
| `Up` / `Down` / `PageUp` / `PageDown` | Navigate within the active list |
| `Home` / `End` | Jump to first/last item |
| `Enter` / `Space` | Toggle the selected asset |
| `/` | Enter search mode (type query, press Enter to apply, Esc to cancel) |
| `Ctrl+S` | Save the enablement file (validates against schema) |
| `r` | Reload catalog + enablement from disk (prompts if dirty) |
| `c` | Remove orphaned enablement entries |
| `q` | Quit (prompts if unsaved changes) |
| `Esc` | Clear active message and filter |

### Output Files

Saves are written to `data/enablement.json` using atomic updates. The file is validated against the bundled schema at `docs/schemas/enablement.schema.json` before writing.

## Development

Run the full test suite:

```bash
cargo test --package awesome-copilot-tui
```

Useful commands while developing:

```bash
cargo fmt
cargo clippy --all-targets --all-features
```

## Project Layout

- `src/app.rs` — CLI parsing, runtime loop, event handling
- `src/domain/` — Catalog models, effective state computation, toggle logic
- `src/io/` — File loaders, schema validation, path helpers
- `src/ui/` — Ratatui rendering helpers and layout
- `tests/` — Integration tests (future expansion)

The detailed requirements and design are tracked in `requirements.md`, `design.md`, and `tasks.md` within this directory.
