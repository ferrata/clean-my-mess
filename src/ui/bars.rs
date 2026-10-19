use ratatui::{
    style::{Color, Style},
    text::Span,
};

use crate::theme::MUTED;

pub(crate) fn smooth_bar(ratio: f64, width: usize, color: Color) -> Vec<Span<'static>> {
    const PARTIAL: [&str; 8] = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];
    let mut eighths = (ratio.clamp(0.0, 1.0) * width as f64 * 8.0).round() as usize;
    if ratio > 0.0 {
        eighths = eighths.max(1);
    }
    let (full, rem) = (eighths / 8, eighths % 8);
    let used = full + usize::from(rem > 0);
    vec![
        Span::styled(format!("{}{}", "█".repeat(full), PARTIAL[rem]), Style::new().fg(color)),
        Span::styled("·".repeat(width - used), Style::new().fg(MUTED)),
    ]
}
