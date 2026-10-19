mod bars;
mod footer;
mod legend;
mod popup;

use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::{
    app::{App, Mode},
    format::{human, plural, spinner},
    theme::{kind_color, BAR_WIDTH, MUTED},
};
use bars::{split_bar, stacked_bar};
use footer::footer_line;
use legend::legend;
use popup::{confirm_popup, progress_popup};

pub(crate) fn draw(f: &mut Frame, app: &App) {
    let [header, body, footer_area] =
        Layout::vertical([Constraint::Length(4), Constraint::Min(3), Constraint::Length(1)]).areas(f.area());

    let totals = app.kind_totals();
    let scan_state = if !app.scanning {
        "scan done".to_string()
    } else if app.view_complete() {
        format!("✓ this folder done · {} scanning elsewhere… {} dirs", spinner(app.started), app.visited)
    } else {
        format!("{} scanning… {} dirs", spinner(app.started), app.visited)
    };
    let block = Block::bordered().title(format!(" {} · {} ", app.cwd.display(), scan_state));
    let inner = block.inner(header);
    f.render_widget(block, header);
    let (legend_lines, bar_totals) = legend(&totals, inner.width as usize, 1);
    let mut header_lines = vec![stacked_bar(&bar_totals, inner.width as usize)];
    header_lines.extend(legend_lines);
    f.render_widget(Paragraph::new(header_lines), inner);

    let max = app.entries.first().map(|e| e.size).unwrap_or(1).max(1);
    let rows: Vec<ListItem> = app
        .entries
        .iter()
        .map(|entry| {
            let mark = if app.selected.contains(&entry.path) { "[x]" } else { "[ ]" };
            let kind = entry.kind.unwrap_or_else(|| entry.dominant());
            if entry.count == 0 {
                let label = if app.entry_pending(entry) {
                    format!("{} scanning…", spinner(app.started))
                } else {
                    "clean".into()
                };
                let dim = Style::new().fg(MUTED);
                return ListItem::new(Line::from(vec![
                    Span::raw(format!("{mark} ")),
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
        })
        .collect();
    let scope_total: u64 = app.scope_items().map(|i| i.size).sum();
    let title = format!(
        " {} here · {} · {} marked ({}) · {} freed ",
        plural(app.scope_items().count(), "waste folder"),
        human(scope_total),
        app.selected.len(),
        human(app.selected_size()),
        human(app.freed),
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

    match app.mode {
        Mode::Confirm => confirm_popup(f, app),
        Mode::Deleting => progress_popup(f, app),
        Mode::Normal => {}
    }
}
