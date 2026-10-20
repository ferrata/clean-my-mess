use ratatui::{
    layout::Rect,
    widgets::{Block, Paragraph},
    Frame,
};

use crate::{
    app::App,
    format::spinner,
    ui::{bars::stacked_bar, legend::legend},
};

fn scan_state(app: &App) -> String {
    if !app.scanning {
        "scan done".to_string()
    } else if app.view_complete() {
        format!("✓ this folder done · {} scanning elsewhere… {} dirs", spinner(app.started), app.visited)
    } else {
        format!("{} scanning… {} dirs", spinner(app.started), app.visited)
    }
}

pub(super) fn draw(f: &mut Frame, area: Rect, app: &App) {
    let totals = app.kind_totals();
    let block = Block::bordered().title(format!(" {} · {} ", app.cwd.display(), scan_state(app)));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let (legend_lines, bar_totals) = legend(&totals, inner.width as usize, 1);
    let mut header_lines = vec![stacked_bar(&bar_totals, inner.width as usize)];
    header_lines.extend(legend_lines);
    f.render_widget(Paragraph::new(header_lines), inner);
}
