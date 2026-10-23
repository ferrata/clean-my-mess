use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState},
    Frame,
};

use crate::{
    app::App,
    format::{human, plural, spinner},
    theme::{kind_color, BAR_WIDTH, MUTED},
    tree::Entry,
    ui::bars::split_bar,
};

fn clean_row(app: &App, entry: &Entry, mark: &str) -> ListItem<'static> {
    let label = if app.entry_pending(entry) { format!("{} scanning…", spinner(app.started)) } else { "clean".into() };
    let dim = Style::new().fg(MUTED);
    ListItem::new(Line::from(vec![
        Span::raw(format!("{mark} ")),
        Span::styled("·".repeat(BAR_WIDTH), dim),
        Span::styled(format!("   {:>9}  {label:<14} {}/", "–", entry.name), dim),
    ]))
}

fn waste_row(app: &App, entry: &Entry, mark: &str, max: u64) -> ListItem<'static> {
    let kind = entry.kind.unwrap_or_else(|| entry.dominant());
    let (label, name) = match entry.kind {
        Some(k) => (k.to_string(), entry.name.clone()),
        None => (plural(entry.count, "waste dir"), format!("{}/", entry.name)),
    };
    let name_style = if entry.is_waste() { Style::new() } else { Style::new().bold() };
    let label_style = if entry.is_waste() { Style::new().fg(kind_color(kind)) } else { Style::new().fg(MUTED) };
    let mut spans = vec![Span::raw(format!("{mark} "))];
    spans.extend(split_bar(entry.size as f64 / max as f64, BAR_WIDTH, &entry.by_kind));
    let progress = if app.entry_pending(entry) { format!(" {}", spinner(app.started)) } else { "  ".into() };
    spans.extend([
        Span::styled(progress, Style::new().fg(MUTED)),
        Span::raw(format!(" {:>9}  ", human(entry.size))),
        Span::styled(format!("{label:<14} "), label_style),
        Span::styled(name, name_style),
    ]);
    ListItem::new(Line::from(spans))
}

fn entry_row(app: &App, entry: &Entry, max: u64) -> ListItem<'static> {
    let mark = if app.selected.contains(&entry.path) { "[x]" } else { "[ ]" };
    if entry.count == 0 {
        clean_row(app, entry, mark)
    } else {
        waste_row(app, entry, mark, max)
    }
}

fn list_title(app: &App) -> String {
    let scope_total: u64 = app.scope_items().map(|i| i.size).sum();
    let hidden = app.hidden_marks();
    let elsewhere = if hidden > 0 { format!(" +{hidden} elsewhere") } else { String::new() };
    format!(
        " {} here · {} · {} marked ({}){elsewhere} · {} freed ",
        plural(app.scope_items().count(), "waste folder"),
        human(scope_total),
        app.visible_marked().len(),
        human(app.selected_size()),
        human(app.freed),
    )
}

pub(super) fn draw(f: &mut Frame, area: Rect, app: &App) {
    let max = app.entries.first().map(|e| e.size).unwrap_or(1).max(1);
    let rows: Vec<ListItem> = app.entries.iter().map(|entry| entry_row(app, entry, max)).collect();
    let list = List::new(rows)
        .block(Block::bordered().title(list_title(app)))
        .highlight_symbol("▶ ")
        .highlight_style(Style::new().bg(Color::Indexed(238)).add_modifier(Modifier::BOLD));
    let mut state = ListState::default();
    if !app.entries.is_empty() {
        state.select(Some(app.cursor));
    }
    f.render_stateful_widget(list, area, &mut state);
}
