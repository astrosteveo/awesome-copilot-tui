use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState, Tabs, Wrap},
    Frame,
};

use crate::{
    app::{App, PendingPrompt},
    domain::model::AssetKind,
};

use super::{components, input};

pub fn render(frame: &mut Frame<'_>, app: &App) {
    let size = frame.size();
    if size.width < 50 || size.height < 20 {
        frame.render_widget(
            Paragraph::new("Terminal too small for UI (min 50x20)")
                .style(Style::default().fg(Color::Red)),
            size,
        );
        return;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Min(7),
            Constraint::Length(2),
        ])
        .split(size);

    render_header(frame, layout[0], app);
    render_tabs(frame, layout[1], app);
    render_body(frame, layout[2], app);
    render_footer(frame, layout[3], app);
}

fn render_header(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let repo = app.repo_root().display().to_string();
    let dirty = if app.dirty() {
        Span::styled(
            "DIRTY",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw("clean")
    };
    let filter = app.search_state();
    let filter_text = if filter.query().is_empty() {
        "(none)".to_string()
    } else {
        filter.query().to_string()
    };
    let line = Line::from(vec![
        Span::styled(repo, Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" | Tab:"),
        Span::styled(tab_title(app.tab()), Style::default().fg(Color::Cyan)),
        Span::raw(" | "),
        dirty,
        Span::raw(" | Filter:"),
        Span::raw(filter_text),
        Span::raw(" | Orphans:"),
        Span::raw(app.orphan_count().to_string()),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

const TABS: [AssetKind; 4] = [
    AssetKind::Prompt,
    AssetKind::Instruction,
    AssetKind::ChatMode,
    AssetKind::Collection,
];

fn render_tabs(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let titles = TABS
        .iter()
        .map(|kind| Line::from(tab_title(*kind)))
        .collect::<Vec<_>>();
    let selected = TABS.iter().position(|kind| *kind == app.tab()).unwrap_or(0);
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title("Kinds"))
        .select(selected)
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(tabs, area);
}

fn render_body(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    render_table(frame, body[0], app);
    render_detail(frame, body[1], app);
}

fn render_table(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let assets = app.current_assets();
    let rows: Vec<Row> = assets
        .iter()
        .map(|asset| {
            let state_cell = {
                let badge = components::state_badge(asset);
                let style = if asset.effective {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Red)
                };
                Cell::from(badge).style(style)
            };
            
            Row::new(vec![
                state_cell,
                Cell::from(asset.name.clone()),
                Cell::from(asset.path.clone()),
                Cell::from(components::local_status(asset)),
                Cell::from(components::tags_field(asset)),
            ])
        })
        .collect();

    let header = Row::new(vec!["State", "Name", "Path", "Local", "Tags"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let widths = [
        Constraint::Length(8),
        Constraint::Percentage(30),
        Constraint::Percentage(40),
        Constraint::Length(8),
        Constraint::Percentage(22),
    ];
    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title("Assets"))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    let mut state = TableState::default();
    if let Some(index) = app.selection_index() {
        state.select(Some(index));
    }
    frame.render_stateful_widget(table, area, &mut state);
}

fn render_detail(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let block = Block::default().borders(Borders::ALL).title("Details");
    if let Some(asset) = app.selected_asset_view() {
        let mut lines = Vec::new();
        lines.push(Line::from(vec![
            Span::styled(&asset.name, Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" "),
            Span::styled(tab_title(asset.kind), Style::default().fg(Color::Cyan)),
        ]));
        lines.push(Line::from(format!("Path: {}", asset.path)));
        if let Some(slug) = &asset.slug {
            lines.push(Line::from(format!("Slug: {slug}")));
        }
        lines.push(Line::from(components::status_line(asset)));
        lines.push(Line::from(format!(
            "Collections: {}",
            components::collections_list(asset)
        )));
        if !asset.tags.is_empty() {
            lines.push(Line::from(format!("Tags: {}", asset.tags.join(", "))));
        }
        if !asset.apply_to.is_empty() {
            lines.push(Line::from(format!(
                "applyTo: {}",
                asset.apply_to.join(" | ")
            )));
        }
        if !asset.tools.is_empty() {
            lines.push(Line::from(format!("Tools: {}", asset.tools.join(", "))));
        }
        if asset.kind == AssetKind::Collection {
            lines.push(Line::from(format!("Members: {}", asset.member_count)));
        }
        
        // Add Toggle Preview section
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Toggle Preview:",
            Style::default().add_modifier(Modifier::BOLD).fg(Color::Yellow)
        )));
        for line in components::toggle_preview(asset).lines() {
            lines.push(Line::from(line.to_string()));
        }
        
        // For collections, show impact analysis
        if asset.kind == AssetKind::Collection {
            if let Some(impact) = components::collection_toggle_impact(asset, app.domain()) {
                lines.push(Line::from(""));
                for line in impact.lines() {
                    lines.push(Line::from(line.to_string()));
                }
            }
        }
        
        lines.push(Line::from(""));
        for line in asset.description.lines() {
            lines.push(Line::from(line.to_string()));
        }

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true }).block(block);
        frame.render_widget(paragraph, area);
    } else {
        frame.render_widget(Paragraph::new("No asset selected").block(block), area);
    }
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let footer_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    let mut spans = Vec::new();
    if let Some(err) = app.error() {
        spans.push(Span::styled(
            format!("Error: {err}"),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
    } else if let Some(info) = app.info_message() {
        spans.push(Span::styled(info, Style::default().fg(Color::Green)));
    }

    if !app.warnings().is_empty() {
        if !spans.is_empty() {
            spans.push(Span::raw(" | "));
        }
        spans.push(Span::styled(
            format!("Warnings: {}", app.warnings().join("; ")),
            Style::default().fg(Color::Yellow),
        ));
    }

    if let Some(prompt) = app.prompt() {
        if !spans.is_empty() {
            spans.push(Span::raw(" | "));
        }
        spans.push(Span::styled(
            prompt_text(prompt),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    }

    let line = if spans.is_empty() {
        Line::from(" ")
    } else {
        Line::from(spans)
    };
    frame.render_widget(Paragraph::new(line), footer_layout[0]);

    let search = app.search_state();
    if search.is_active() {
        let prompt = format!("Search > {}_", search.draft());
        frame.render_widget(
            Paragraph::new(prompt).style(Style::default().fg(Color::Cyan)),
            footer_layout[1],
        );
    } else {
        let hints = format!("{}  |  a=Apply from upstream", input::key_hints());
        frame.render_widget(Paragraph::new(hints), footer_layout[1]);
    }
}

fn tab_title(kind: AssetKind) -> &'static str {
    match kind {
        AssetKind::Prompt => "Prompts",
        AssetKind::Instruction => "Instructions",
        AssetKind::ChatMode => "Chat Modes",
        AssetKind::Collection => "Collections",
    }
}

fn prompt_text(prompt: PendingPrompt) -> &'static str {
    match prompt {
        PendingPrompt::Quit => "Confirm quit: y=Yes / n=No",
        PendingPrompt::Reload => "Confirm reload (discard changes): y=Yes / n=No",
        PendingPrompt::ToggleCollection => "Confirm collection toggle: y=Yes / n=No",
    }
}
