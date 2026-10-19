use ratatui::{
    style::Style,
    text::{Line, Span},
};

use crate::theme::{kind_color, MUTED};

pub(crate) fn split_bar(ratio: f64, width: usize, parts: &[(&'static str, u64)]) -> Vec<Span<'static>> {
    const PARTIAL: [&str; 8] = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];
    if parts.is_empty() {
        return vec![Span::styled("·".repeat(width), Style::new().fg(MUTED))];
    }
    let mut parts = parts.to_vec();
    parts.sort_by(|a, b| b.1.cmp(&a.1));
    let total: u64 = parts.iter().map(|p| p.1).sum();
    let mut eighths = (ratio.clamp(0.0, 1.0) * width as f64 * 8.0).round() as usize;
    if ratio > 0.0 {
        eighths = eighths.max(1);
    }
    let (full, rem) = (eighths / 8, eighths % 8);
    let cells = full + usize::from(rem > 0);

    let mut counts: Vec<usize> = parts
        .iter()
        .map(|(_, size)| ((*size as f64 / total.max(1) as f64 * cells as f64).round() as usize).max(1))
        .collect();
    counts.truncate(cells);
    while counts.iter().sum::<usize>() > cells {
        if let Some(biggest) = (0..counts.len()).filter(|&i| counts[i] > 1).max_by_key(|&i| counts[i]) {
            counts[biggest] -= 1;
        } else {
            break;
        }
    }
    while counts.iter().sum::<usize>() < cells {
        counts[0] += 1;
    }
    let cell_kinds: Vec<&str> =
        counts.iter().enumerate().flat_map(|(i, n)| std::iter::repeat(parts[i].0).take(*n)).collect();

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut last_kind = "";
    for (i, kind) in cell_kinds.iter().enumerate() {
        let glyph = if i < full { "█" } else { PARTIAL[rem] };
        if *kind == last_kind && i < full {
            if let Some(span) = spans.last_mut() {
                span.content.to_mut().push_str(glyph);
                continue;
            }
        }
        spans.push(Span::styled(glyph.to_string(), Style::new().fg(kind_color(kind))));
        last_kind = if i < full { kind } else { "" };
    }
    spans.push(Span::styled("·".repeat(width - cells), Style::new().fg(MUTED)));
    spans
}

pub(crate) fn stacked_bar(totals: &[(&'static str, u64)], width: usize) -> Line<'static> {
    let sum: u64 = totals.iter().map(|t| t.1).sum();
    if sum == 0 {
        return Line::from(Span::styled("·".repeat(width), Style::new().fg(MUTED)));
    }
    let mut spans = Vec::new();
    let (mut acc, mut drawn) = (0u64, 0usize);
    for (kind, size) in totals {
        acc += size;
        let end = (acc as f64 / sum as f64 * width as f64).round() as usize;
        if end > drawn {
            spans.push(Span::styled("█".repeat(end - drawn), Style::new().fg(kind_color(kind))));
            drawn = end;
        }
    }
    Line::from(spans)
}
