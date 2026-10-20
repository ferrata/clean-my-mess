mod bars;
mod footer;
mod header;
mod legend;
mod list;
mod popup;
mod side;

use ratatui::{
    layout::{Constraint, Layout},
    widgets::Paragraph,
    Frame,
};

use crate::app::App;
use footer::footer_line;

pub(crate) fn draw(f: &mut Frame, app: &App) {
    let [header, body, footer_area] =
        Layout::vertical([Constraint::Length(4), Constraint::Min(3), Constraint::Length(1)]).areas(f.area());
    let (list_area, side_area) = side::split(body);

    header::draw(f, header, app);

    list::draw(f, list_area, app);

    if let Some(side_area) = side_area {
        side::draw_drive(f, side_area, app);
    }

    f.render_widget(Paragraph::new(footer_line(app)), footer_area);

    popup::draw(f, app);
}
