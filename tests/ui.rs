use std::path::PathBuf;

use clean_my_mess::{scan::Item, App};
use ratatui::{crossterm::event::KeyCode, style::Color};

const KIB: u64 = 1024;
const MIB: u64 = 1024 * KIB;

fn item(path: &str, kind: &'static str, size: u64) -> Item {
    Item { path: PathBuf::from(path), kind, size }
}

fn sample() -> App {
    App::with_items(
        PathBuf::from("/proj"),
        vec![
            item("/proj/a/node_modules", "Node", 3 * MIB),
            item("/proj/a/.next", "Next.js", MIB),
            item("/proj/b/target", "Rust", 9 * MIB),
            item("/proj/c/node_modules", "Node", 100 * KIB),
        ],
    )
}

fn row_ends_with(line: &str, suffix: &str) -> bool {
    line.trim_end_matches(['│', ' ']).ends_with(suffix)
}

fn row<'a>(screen: &'a str, name: &str) -> &'a str {
    let suffix = format!(" {name}");
    screen
        .lines()
        .find(|l| row_ends_with(l, &suffix))
        .unwrap_or_else(|| panic!("no row ending with {suffix:?} in:\n{screen}"))
}

fn row_index(screen: &str, name: &str) -> usize {
    let suffix = format!(" {name}");
    screen.lines().position(|l| row_ends_with(l, &suffix)).expect("row present")
}

fn press(app: &mut App, keys: &[KeyCode]) {
    for key in keys {
        app.press(*key);
    }
}

#[test]
fn root_lists_directories_largest_first() {
    let screen = sample().render(100, 24);
    assert!(row_index(&screen, "b/") < row_index(&screen, "a/"), "{screen}");
    assert!(row_index(&screen, "a/") < row_index(&screen, "c/"), "{screen}");
    assert!(row(&screen, "a/").contains("2 waste dirs"));
    assert!(row(&screen, "b/").contains("1 waste dir "));
    assert!(row(&screen, "b/").contains("9.0 MB"));
    assert!(screen.contains("4 waste folders here"));
}

#[test]
fn empty_state_is_explained() {
    let screen = App::with_items(PathBuf::from("/proj"), vec![]).render(100, 24);
    assert!(screen.contains("No waste in this directory"), "{screen}");
}

#[test]
fn right_opens_directory_and_left_goes_back() {
    let mut app = sample();
    press(&mut app, &[KeyCode::Down, KeyCode::Right]);
    let inside = app.render(100, 24);
    assert!(inside.contains("/proj/a"), "{inside}");
    assert!(row(&inside, "node_modules").contains("Node"));
    assert!(row(&inside, ".next").contains("Next.js"));

    app.press(KeyCode::Left);
    let back = app.render(100, 24);
    assert!(row(&back, "a/").contains("2 waste dirs"), "{back}");
    assert!(!back.contains("/proj/a "), "{back}");
}

#[test]
fn waste_folder_itself_cannot_be_opened() {
    let mut app = sample();
    press(&mut app, &[KeyCode::Right, KeyCode::Right]);
    let screen = app.render(100, 24);
    assert!(screen.contains("This is a waste folder itself"), "{screen}");
}

fn many_kinds() -> App {
    let kinds = [
        "Node",
        "Next.js",
        "Rust",
        "Python",
        "Python venv",
        "JS build",
        "Gradle",
        "Xcode",
        "Dart",
        "Terraform",
        "Maven",
    ];
    let items = kinds
        .iter()
        .enumerate()
        .map(|(i, kind)| item(&format!("/proj/p/dir{i}"), kind, (20 - i as u64) * MIB))
        .collect();
    App::with_items(PathBuf::from("/proj"), items)
}

#[test]
fn legend_folds_small_types_into_other_and_never_overflows() {
    let app = many_kinds();
    let mut markers = Vec::new();
    for width in [160u16, 120, 80, 56] {
        let screen = app.render(width, 24);
        let legend = screen.lines().nth(2).expect("legend line");
        assert!(legend.contains("other ("), "width {width}: {legend}");
        assert!(legend.chars().count() <= width as usize, "width {width}: {legend}");
        markers.push(legend.matches('■').count());
    }
    assert!(markers.windows(2).all(|w| w[0] >= w[1]), "narrower screens show fewer types: {markers:?}");
    assert!(markers[0] > markers[3], "{markers:?}");
}

fn bar_colors(app: &App, name: &str) -> Vec<Color> {
    let buffer = app.render_buffer(100, 24);
    let screen = app.render(100, 24);
    let y = row_index(&screen, name) as u16;
    let mut colors = Vec::new();
    for x in 0..100 {
        let cell = &buffer[(x, y)];
        if cell.symbol() == "█" && !colors.contains(&cell.fg) {
            colors.push(cell.fg);
        }
    }
    colors
}

#[test]
fn directory_bar_is_split_by_type() {
    let app = App::with_items(
        PathBuf::from("/proj"),
        vec![
            item("/proj/mixed/node_modules", "Node", 6 * MIB),
            item("/proj/mixed/target", "Rust", 3 * MIB),
            item("/proj/mixed/.next", "Next.js", 3 * MIB),
            item("/proj/pure/node_modules", "Node", 2 * MIB),
        ],
    );
    assert_eq!(bar_colors(&app, "mixed/").len(), 3);
    assert_eq!(bar_colors(&app, "pure/").len(), 1);
}

#[test]
fn small_items_still_show_a_sliver() {
    let app = App::with_items(
        PathBuf::from("/proj"),
        vec![item("/proj/big/node_modules", "Node", 500 * MIB), item("/proj/tiny/node_modules", "Node", 4 * KIB)],
    );
    let screen = app.render(100, 24);
    assert!(row(&screen, "tiny/").contains('▏'), "{screen}");
}

#[test]
fn tiny_terminals_do_not_panic() {
    for (w, h) in [(1, 1), (10, 3), (20, 6), (40, 8), (50, 12)] {
        let mut app = many_kinds();
        app.render(w, h);
        app.press(KeyCode::Char('d'));
        app.render(w, h);
        app.press(KeyCode::Right);
        app.render(w, h);
    }
}
