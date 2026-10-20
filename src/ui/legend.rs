use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

use crate::{
    format::human,
    theme::{kind_color, MUTED},
};

fn fits_lines(lens: &[usize], width: usize, max_lines: usize) -> bool {
    let (mut lines, mut used) = (1, 0usize);
    for &len in lens {
        let need = if used == 0 { len } else { used + 2 + len };
        if need <= width {
            used = need;
        } else {
            lines += 1;
            used = len;
        }
    }
    lines <= max_lines && lens.iter().all(|&l| l <= width)
}

pub(super) fn legend(
    totals: &[(&'static str, u64)],
    width: usize,
    max_lines: usize,
) -> (Vec<Line<'static>>, Vec<(&'static str, u64)>) {
    let item_len = |label: &str, size: u64| 2 + label.chars().count() + 1 + human(size).len();
    let other_label = |rest: &[(&'static str, u64)]| format!("other ({})", rest.len());
    let other_len = |rest: &[(&'static str, u64)]| item_len(&other_label(rest), rest.iter().map(|r| r.1).sum());

    let keep = (0..=totals.len())
        .rev()
        .find(|&k| {
            let mut lens: Vec<usize> = totals[..k].iter().map(|(kind, size)| item_len(kind, *size)).collect();
            if k < totals.len() {
                lens.push(other_len(&totals[k..]));
            }
            fits_lines(&lens, width, max_lines)
        })
        .unwrap_or(0);

    let mut items: Vec<(String, u64, Color)> =
        totals[..keep].iter().map(|(kind, size)| (kind.to_string(), *size, kind_color(kind))).collect();
    let mut bar_totals = totals[..keep].to_vec();
    if keep < totals.len() {
        let rest = &totals[keep..];
        let sum: u64 = rest.iter().map(|r| r.1).sum();
        items.push((other_label(rest), sum, MUTED));
        bar_totals.push(("other", sum));
    }

    let mut lines: Vec<Vec<Span<'static>>> = vec![Vec::new()];
    let mut used = 0usize;
    for (label, size, color) in items {
        let len = item_len(&label, size);
        if used > 0 && used + 2 + len > width {
            lines.push(Vec::new());
            used = 0;
        }
        if used > 0 {
            lines.last_mut().into_iter().for_each(|l| l.push(Span::raw("  ")));
            used += 2;
        }
        if let Some(line) = lines.last_mut() {
            line.push(Span::styled("■ ", Style::new().fg(color)));
            line.push(Span::raw(format!("{label} {}", human(size))));
        }
        used += len;
    }
    (lines.into_iter().map(Line::from).collect(), bar_totals)
}
