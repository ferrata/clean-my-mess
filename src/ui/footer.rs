use ratatui::{
    style::{Color, Style, Stylize},
    text::{Line, Span},
};

use crate::{
    app::App,
    format::{human, plural},
};

fn key(label: &str, color: Color) -> Span<'static> {
    Span::styled(label.to_string(), Style::new().fg(color).bold())
}

pub(crate) fn footer_line(app: &App) -> Line<'static> {
    if !app.status.is_empty() {
        return Line::from(app.status.clone()).fg(Color::Green);
    }
    let Some(entry) = app.entries.get(app.cursor) else {
        let msg = if !app.view_complete() { "Scanning for waste…" } else { "No waste in this directory." };
        return Line::from(vec![Span::raw(msg), Span::raw("  "), key("←", Color::Yellow), Span::raw(" up  q quit")]);
    };
    let marked = app.visible_marked().len();
    if marked > 0 {
        return Line::from(vec![
            key("d", Color::Red),
            Span::raw(format!(" delete {} marked ({}, asks first)  ", marked, human(app.selected_size()))),
            key("space", Color::Yellow),
            Span::raw(" toggle  "),
            key("a", Color::Yellow),
            Span::raw(" mark all  "),
            key("n", Color::Yellow),
            Span::raw(" unmark  q quit"),
        ]);
    }
    if entry.count == 0 {
        let note = if app.entry_pending(entry) { "no waste found yet (still scanning)" } else { "clean" };
        return Line::from(vec![
            Span::raw(format!("{note}  ")),
            key("→", Color::Yellow),
            Span::raw(" open  "),
            key("←", Color::Yellow),
            Span::raw(" up  q quit"),
        ]);
    }
    let what = if entry.is_waste() {
        format!(" delete this {} folder ", entry.kind.unwrap_or(""))
    } else {
        let partial = if app.entry_pending(entry) { ", still scanning" } else { "" };
        format!(
            " delete all waste under {}/ ({}, {}{partial}) ",
            entry.name,
            plural(entry.count, "folder"),
            human(entry.size)
        )
    };
    let mut spans = vec![key("d", Color::Red), Span::raw(what), Span::raw("(asks first)  ")];
    if !entry.is_waste() {
        spans.extend([key("→", Color::Yellow), Span::raw(" open  ")]);
    }
    spans.extend([
        key("←", Color::Yellow),
        Span::raw(" up  "),
        key("space", Color::Yellow),
        Span::raw(" mark  "),
        key("a", Color::Yellow),
        Span::raw(" mark all  q quit"),
    ]);
    Line::from(spans)
}
