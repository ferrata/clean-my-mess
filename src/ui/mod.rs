mod bars;
mod footer;

use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::{
    app::App,
    format::{human, plural, spinner},
    theme::{kind_color, BAR_WIDTH, MUTED},
};
use bars::smooth_bar;
use footer::footer_line;

pub(crate) fn draw(f: &mut Frame, app: &App) {
    let [body, footer_area] = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).areas(f.area());

    let max = app.entries.first().map(|e| e.size).unwrap_or(1).max(1);
    let rows: Vec<ListItem> = app
        .entries
        .iter()
        .map(|entry| {
            let kind = entry.kind.unwrap_or_else(|| entry.dominant());
            if entry.count == 0 {
                let label = if app.scanning { format!("{} scanning…", spinner(app.started)) } else { "clean".into() };
                let dim = Style::new().fg(MUTED);
                return ListItem::new(Line::from(vec![
                    Span::styled("·".repeat(BAR_WIDTH), dim),
                    Span::styled(format!(" {:>9}  {label:<14} {}/", "–", entry.name), dim),
                ]));
            }
            let (label, name) = match entry.kind {
                Some(k) => (k.to_string(), entry.name.clone()),
                None => (plural(entry.count, "waste dir"), format!("{}/", entry.name)),
            };
            let name_style = if entry.is_waste() { Style::new() } else { Style::new().bold() };
            let label_style = if entry.is_waste() { Style::new().fg(kind_color(kind)) } else { Style::new().fg(MUTED) };
            let mut spans = smooth_bar(entry.size as f64 / max as f64, BAR_WIDTH, kind_color(kind));
            spans.extend([
                Span::raw(format!(" {:>9}  ", human(entry.size))),
                Span::styled(format!("{label:<14} "), label_style),
                Span::styled(name, name_style),
            ]);
            ListItem::new(Line::from(spans))
        })
        .collect();
    let scope_total: u64 = app.scope_items().map(|i| i.size).sum();
    let scan_state = if app.scanning {
        format!("{} scanning… {} dirs", spinner(app.started), app.visited)
    } else {
        "scan done".into()
    };
    let title = format!(
        " {} · {} · {} here · {} ",
        app.cwd.display(),
        scan_state,
        plural(app.scope_items().count(), "waste folder"),
        human(scope_total),
    );
    let list = List::new(rows)
        .block(Block::bordered().title(title))
        .highlight_symbol("▶ ")
        .highlight_style(Style::new().bg(Color::Indexed(238)).add_modifier(Modifier::BOLD));
    let mut state = ListState::default();
    if !app.entries.is_empty() {
        state.select(Some(app.cursor));
    }
    f.render_stateful_widget(list, body, &mut state);
    f.render_widget(Paragraph::new(footer_line(app)), footer_area);
}
