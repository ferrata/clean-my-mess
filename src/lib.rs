pub mod delete;
pub mod disk;
pub mod scan;

mod app;
mod format;
mod theme;
mod tree;
mod ui;

use std::{fs, io, path::PathBuf, time::Duration};

use ratatui::{
    crossterm::event::{self, Event, KeyEventKind},
    DefaultTerminal,
};

pub use app::{App, Choice};
pub use format::{estimate_remaining, format_duration};

fn run(terminal: &mut DefaultTerminal, mut app: App) -> io::Result<()> {
    while !app.quit {
        app.pump();
        terminal.draw(|f| ui::draw(f, &app))?;
        if event::poll(Duration::from_millis(80))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    app.on_key(key.code, key.modifiers);
                    app.refresh();
                }
            }
        }
    }
    Ok(())
}

pub fn run_in_terminal(root: PathBuf) -> io::Result<()> {
    let root = fs::canonicalize(&root).map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", root.display())))?;
    let mut terminal = ratatui::init();
    let result = run(&mut terminal, App::new(root));
    ratatui::restore();
    result
}
