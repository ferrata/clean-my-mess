use ratatui::style::Color;

pub(crate) const KIND_COLORS: &[(&str, u8)] = &[
    ("Node", 71),
    ("Next.js", 140),
    ("Nuxt", 72),
    ("SvelteKit", 167),
    ("Angular", 132),
    ("Turborepo", 175),
    ("Parcel", 138),
    ("JS build", 179),
    ("Coverage", 244),
    ("Rust", 173),
    ("Maven", 95),
    ("Gradle", 66),
    ("Python", 68),
    ("Python venv", 110),
    ("CocoaPods", 133),
    ("Xcode", 61),
    ("Dart", 80),
    ("Terraform", 101),
];

pub(crate) const MUTED: Color = Color::Indexed(245);
pub(crate) const BACKDROP: Color = Color::Indexed(240);
pub(crate) const SHADOW: Color = Color::Indexed(233);

pub(crate) const BAR_WIDTH: usize = 12;

pub(crate) fn kind_color(kind: &str) -> Color {
    let index = KIND_COLORS.iter().find(|(k, _)| *k == kind).map(|(_, c)| *c).unwrap_or(245);
    Color::Indexed(index)
}
