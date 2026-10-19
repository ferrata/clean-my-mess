mod common;

use std::time::{Duration, Instant};

use clean_my_mess::App;
use common::TempDir;
use ratatui::crossterm::event::KeyCode;

fn pump_until(app: &mut App, what: &str, done: impl Fn(&App, &str) -> bool) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        app.pump();
        let screen = app.render(110, 24);
        if done(app, &screen) {
            return screen;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}:\n{screen}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn tree() -> TempDir {
    let dir = TempDir::new();
    dir.file("top/clean/a/b/readme.txt", 10);
    dir.file("top/proj/package.json", 2);
    dir.file("top/proj/node_modules/pkg/index.js", 5000);
    dir
}

#[test]
fn clean_folder_is_reported_clean_and_results_survive_going_back() {
    let dir = tree();
    let mut app = App::new(dir.0.join("top"));

    app.press(KeyCode::Right);
    let inside = pump_until(&mut app, "clean subtree to finish", |_, s| {
        s.lines().any(|l| l.trim_end_matches([' ', '│']).ends_with(" a/") && l.contains("clean"))
    });
    assert!(inside.contains("top/clean"), "{inside}");

    app.press(KeyCode::Left);
    let back = pump_until(&mut app, "parent to show the waste", |a, s| !a.is_scanning() && s.contains("1 waste dir"));
    assert!(back.contains("1 waste folder here"), "no duplicates after going back:\n{back}");
}

#[test]
fn going_above_the_root_keeps_what_was_found() {
    let dir = tree();
    dir.file("sibling/node_modules/x", 3000);
    let mut app = App::new(dir.0.join("top"));
    pump_until(&mut app, "first scan", |a, s| !a.is_scanning() && s.contains("1 waste folder here"));

    app.press(KeyCode::Left);
    let screen = pump_until(&mut app, "parent scan", |a, s| !a.is_scanning() && s.contains("2 waste folders here"));
    assert!(
        screen.lines().any(|l| l.trim_end_matches([' ', '│']).ends_with(" top/") && l.contains("1 waste dir")),
        "{screen}"
    );
    assert!(
        screen.lines().any(|l| l.trim_end_matches([' ', '│']).ends_with(" sibling/") && l.contains("1 waste dir")),
        "{screen}"
    );
}
