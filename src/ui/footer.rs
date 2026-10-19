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
        let msg = if app.scanning { "Scanning for waste…" } else { "No waste in this directory." };
        return Line::from(vec![Span::raw(msg), Span::raw("  "), key("←", Color::Yellow), Span::raw(" up  q quit")]);
    };
    if entry.count == 0 {
        let note = if app.scanning { "no waste found yet (still scanning)" } else { "clean" };
        return Line::from(vec![
            Span::raw(format!("{note}  ")),
            key("→", Color::Yellow),
            Span::raw(" open  "),
            key("←", Color::Yellow),
            Span::raw(" up  q quit"),
        ]);
    }
    let mut spans = Vec::new();
    if !entry.is_waste() {
        spans.extend([
            key("→", Color::Yellow),
            Span::raw(format!(" open {}/ ({}, {})  ", entry.name, plural(entry.count, "folder"), human(entry.size))),
        ]);
    }
    spans.extend([key("←", Color::Yellow), Span::raw(" up  q quit")]);
    Line::from(spans)
}
