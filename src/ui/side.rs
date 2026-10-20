use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
    Frame,
};

use crate::{app::App, format::human, theme::MUTED};

fn signed(bytes: i128) -> String {
    let sign = if bytes > 0 {
        "+"
    } else if bytes < 0 {
        "-"
    } else {
        "±"
    };
    format!("{sign}{}", human(bytes.unsigned_abs().min(u128::from(u64::MAX)) as u64))
}

fn usage_color(ratio: f64) -> Color {
    match ratio {
        r if r < 0.75 => Color::Indexed(108),
        r if r < 0.90 => Color::Indexed(179),
        _ => Color::Indexed(167),
    }
}

pub(crate) fn draw_drive(f: &mut Frame, area: Rect, app: &App) {
    let dim = Style::new().fg(MUTED);
    let green = Style::new().fg(Color::Indexed(108));
    let mut lines: Vec<Line<'static>> = Vec::new();

    match app.disk {
        Some(disk) => {
            let ratio = if disk.total == 0 { 0.0 } else { disk.used() as f64 / disk.total as f64 };
            let width = area.width.saturating_sub(4) as usize;
            let filled = ((ratio * width as f64).round() as usize).min(width);
            lines.push(Line::from(vec![
                Span::styled("█".repeat(filled), Style::new().fg(usage_color(ratio))),
                Span::styled("·".repeat(width - filled), dim),
            ]));
            lines.push(Line::from(vec![
                Span::raw(format!("{} used", human(disk.used()))),
                Span::styled(format!(" of {}", human(disk.total)), dim),
            ]));
            lines.push(Line::from(vec![
                Span::styled(human(disk.free), Style::new().bold()),
                Span::styled(format!(" free ({:.0}%)", 100.0 - ratio * 100.0), dim),
            ]));
            lines.push(Line::raw(""));
            lines.push(Line::styled("Free space gained", dim));
            match app.gained {
                Some(gained) => {
                    let style = if gained > 0 { green } else { Style::new() };
                    lines.push(Line::styled(signed(gained), style.bold()));
                    lines.push(Line::styled("statvfs free: after − before", dim));
                }
                None => lines.push(Line::styled("nothing measured yet", dim)),
            }
        }
        None => lines.push(Line::styled("Drive info unavailable", dim)),
    }

    lines.push(Line::raw(""));
    lines.push(Line::styled("Cleared this session", dim));
    lines.push(Line::from(vec![
        Span::styled(human(app.freed), green.bold()),
        Span::styled(" deleted (scan sizes)", dim),
    ]));
    lines.push(Line::from(vec![Span::raw(human(app.trashed)), Span::styled(" in Trash", dim)]));
    if app.trashed > 0 {
        lines.push(Line::styled("(empty Trash to free it)", dim));
    }

    f.render_widget(
        Paragraph::new(lines)
            .block(Block::bordered().title(" Drive ").padding(ratatui::widgets::Padding::horizontal(1))),
        area,
    );
}
