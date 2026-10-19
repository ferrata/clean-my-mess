use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Clear, Padding, Paragraph},
    Frame,
};

use crate::{
    app::{App, Choice},
    format::{human, plural, spinner, truncate},
    theme::{kind_color, BACKDROP, MUTED, SHADOW},
    ui::{bars::stacked_bar, legend::legend},
};

pub(crate) fn button(label: &str, focused: bool, accent: Color) -> Span<'static> {
    let style = if focused {
        Style::new().fg(Color::Black).bg(accent).bold()
    } else {
        Style::new().fg(accent).bg(Color::Indexed(238))
    };
    Span::styled(label.to_string(), style)
}

pub(crate) fn confirm_popup(f: &mut Frame, app: &App) {
    let targets = app.targets();
    let count: usize = targets.iter().map(|e| e.count).sum();
    let size: u64 = targets.iter().map(|e| e.size).sum();

    let mut totals: Vec<(&'static str, u64)> = Vec::new();
    for entry in &targets {
        for (kind, bytes) in &entry.by_kind {
            match totals.iter_mut().find(|(k, _)| k == kind) {
                Some(slot) => slot.1 += bytes,
                None => totals.push((kind, *bytes)),
            }
        }
    }
    totals.sort_by(|a, b| b.1.cmp(&a.1));

    let width = f.area().width.min(76);
    let inner_width = width.saturating_sub(8) as usize;
    let dim = Style::new().fg(MUTED);

    let (legend_lines, bar_totals) = legend(&totals, inner_width, 2);
    let legend_height = legend_lines.len();
    let mut lines = vec![
        Line::from(vec![
            Span::styled(plural(count, "waste folder"), Style::new().bold()),
            Span::styled("  ·  ", dim),
            Span::styled(human(size), Style::new().fg(Color::Indexed(173)).bold()),
        ])
        .centered(),
        Line::raw(""),
        stacked_bar(&bar_totals, inner_width),
    ];
    lines.extend(legend_lines);
    lines.push(Line::raw(""));
    let unfinished = targets.iter().any(|e| app.entry_pending(e));
    let capacity = (f.area().height as usize).saturating_sub(13 + legend_height + usize::from(unfinished)).clamp(2, 6);
    let shown = if targets.len() > capacity { capacity - 1 } else { targets.len() };
    for entry in targets.iter().take(shown) {
        let kind = entry.kind.unwrap_or_else(|| entry.dominant());
        let (name, what) = match entry.kind {
            Some(k) => (entry.name.clone(), k.to_string()),
            None => (format!("{}/", entry.name), plural(entry.count, "folder")),
        };
        let name = truncate(&name, inner_width.saturating_sub(30).max(8));
        let used = 2 + name.chars().count() + 2 + what.chars().count() + 2 + human(entry.size).len();
        let gap = inner_width.saturating_sub(used).max(1);
        lines.push(Line::from(vec![
            Span::styled("■ ", Style::new().fg(kind_color(kind))),
            Span::raw(name),
            Span::styled(format!("  {what}"), dim),
            Span::raw(" ".repeat(gap)),
            Span::raw(human(entry.size)),
        ]));
    }
    if targets.len() > shown {
        lines.push(Line::styled(format!("… and {} more", targets.len() - shown), dim));
    }
    if unfinished {
        lines.push(Line::raw(""));
        lines.push(
            Line::styled(
                "Still scanning here — only folders found so far are listed.",
                Style::new().fg(Color::Indexed(179)),
            )
            .centered(),
        );
    }
    lines.extend([
        Line::raw(""),
        match app.choice {
            Choice::Trash => Line::styled(
                "Moves to the Trash. Space is freed when you empty it.",
                Style::new().fg(Color::Indexed(110)).italic(),
            ),
            Choice::Delete => {
                Line::styled("Deletes for good. This cannot be undone.", Style::new().fg(Color::Indexed(167)).italic())
            }
            Choice::Cancel => Line::styled("Nothing will be removed.", dim.italic()),
        }
        .centered(),
        Line::raw(""),
        Line::from(vec![
            button(" t  Trash ", app.choice == Choice::Trash, Color::Indexed(110)),
            Span::raw("   "),
            button(" y  Delete forever ", app.choice == Choice::Delete, Color::Indexed(167)),
            Span::raw("   "),
            button(" esc  Cancel ", app.choice == Choice::Cancel, MUTED),
        ])
        .centered(),
        Line::raw(""),
        Line::styled("←/→ switch · enter select", dim).centered(),
    ]);

    draw_popup(f, lines, width, " Delete waste ");
}

fn draw_popup(f: &mut Frame, lines: Vec<Line<'static>>, width: u16, title: &'static str) {
    let area = centered(f.area(), width, lines.len() as u16 + 4);
    let overlay = f.area();
    f.buffer_mut().set_style(overlay, Style::new().fg(BACKDROP).remove_modifier(Modifier::BOLD));
    let shadow = Rect::new(area.x + 1, area.y + 1, area.width, area.height).intersection(overlay);
    f.render_widget(Block::new().style(Style::new().fg(SHADOW).bg(SHADOW)), shadow);
    f.render_widget(Clear, area);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(Color::Indexed(167)))
        .title(Line::from(title).fg(Color::Indexed(167)).bold())
        .padding(Padding::new(3, 3, 1, 1))
        .style(Style::new().bg(Color::Indexed(235)));
    f.render_widget(Paragraph::new(lines).block(block), area);
}

pub(crate) fn progress_popup(f: &mut Frame, app: &App) {
    let Some(job) = app.job.as_ref() else { return };
    let width = f.area().width.min(76);
    let inner_width = width.saturating_sub(8) as usize;
    let dim = Style::new().fg(MUTED);
    let accent = Color::Indexed(167);

    let ratio = if job.total_bytes == 0 { 0.0 } else { (job.done_bytes as f64 / job.total_bytes as f64).min(1.0) };
    let filled = (ratio * inner_width as f64).round() as usize;
    let bar = Line::from(vec![
        Span::styled("█".repeat(filled), Style::new().fg(accent)),
        Span::styled("░".repeat(inner_width - filled), Style::new().fg(Color::Indexed(240))),
    ]);
    let current = job.current.strip_prefix(&app.cwd).unwrap_or(&job.current).display().to_string();
    let stop = if job.stopped { " stopping… " } else { " esc  Stop " };

    let lines = vec![
        Line::from(vec![
            Span::styled(format!("{} ", spinner(app.started)), Style::new().fg(accent)),
            Span::styled(
                if job.stopped {
                    "Stopping"
                } else if job.permanent {
                    "Deleting"
                } else {
                    "Moving to Trash"
                },
                Style::new().bold(),
            ),
            Span::styled(format!("  ·  {:.0}%", ratio * 100.0), dim),
        ])
        .centered(),
        Line::raw(""),
        bar,
        Line::raw(""),
        Line::from(format!(
            "{} of {}  ·  {} of {}",
            human(job.done_bytes.min(job.total_bytes)),
            human(job.total_bytes),
            job.done_items,
            plural(job.total_items, "folder"),
        ))
        .centered(),
        Line::styled(truncate(&current, inner_width), dim).centered(),
        Line::raw(""),
        Line::from(button(stop, !job.stopped, Color::Indexed(110))).centered(),
    ];
    draw_popup(f, lines, width, if job.permanent { " Deleting " } else { " Moving to Trash " });
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect::new(area.x + (area.width - w) / 2, area.y + (area.height - h) / 2, w, h)
}
