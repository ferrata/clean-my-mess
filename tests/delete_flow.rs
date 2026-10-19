mod common;

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use clean_my_mess::App;
use common::TempDir;
use ratatui::crossterm::event::KeyCode;

fn settle(app: &mut App, what: &str, done: impl Fn(&App, &str) -> bool) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        app.pump();
        let screen = app.render(160, 24);
        if done(app, &screen) {
            return screen;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}:\n{screen}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn setup() -> (TempDir, PathBuf, App) {
    let dir = TempDir::new();
    dir.file("top/proj/package.json", 2);
    dir.file("top/proj/node_modules/pkg/index.js", 5000);
    let trash = dir.0.join("fake-trash");
    let mut app = App::new(dir.0.join("top"));
    app.set_trash_dir(Some(trash.clone()));
    settle(&mut app, "scan", |a, s| !a.is_scanning() && s.contains("1 waste folder here"));
    (dir, trash, app)
}

#[test]
fn t_moves_the_folder_to_the_trash_and_updates_the_list() {
    let (dir, trash, mut app) = setup();
    app.press(KeyCode::Char('d'));
    app.press(KeyCode::Char('t'));
    let screen = settle(&mut app, "trash to finish", |a, s| !a.dialog_open() && s.contains("Moved to Trash"));
    assert!(screen.contains("0 waste folders here"), "{screen}");
    assert!(screen.contains("space is freed when you empty the Trash"), "{screen}");
    assert!(screen.contains("0 B freed"), "moving to trash frees nothing yet:\n{screen}");
    assert!(!dir.0.join("top/proj/node_modules").exists());
    assert!(trash.join("node_modules (proj)/pkg/index.js").exists());
}

#[test]
fn y_deletes_permanently_and_counts_freed_space() {
    let (dir, trash, mut app) = setup();
    app.press(KeyCode::Char('d'));
    app.press(KeyCode::Char('y'));
    let screen = settle(&mut app, "delete to finish", |a, s| !a.dialog_open() && s.contains("Deleted 1 folder"));
    assert!(!dir.0.join("top/proj/node_modules").exists());
    assert!(!trash.exists(), "permanent delete must not touch the trash");
    assert!(!screen.contains("0 B freed"), "{screen}");
}

#[test]
fn focused_button_with_enter_picks_the_matching_action() {
    let (dir, trash, mut app) = setup();
    app.press(KeyCode::Char('d'));
    app.press(KeyCode::Right);
    app.press(KeyCode::Enter);
    settle(&mut app, "trash to finish", |a, s| !a.dialog_open() && s.contains("Moved to Trash"));
    assert!(trash.join("node_modules (proj)").exists());
    assert!(!dir.0.join("top/proj/node_modules").exists());
}
