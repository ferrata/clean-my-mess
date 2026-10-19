use std::time::{Duration, Instant};

pub(crate) fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub fn estimate_remaining(done: u64, total: u64, elapsed: Duration) -> Option<Duration> {
    let secs = elapsed.as_secs_f64();
    if secs < 1.5 || done == 0 {
        return None;
    }
    let rate = done as f64 / secs;
    Some(Duration::from_secs_f64(total.saturating_sub(done) as f64 / rate))
}

pub fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs_f64().ceil() as u64;
    match secs {
        0 => "<1s".into(),
        1..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m {:02}s", secs / 60, secs % 60),
        _ => format!("{}h {:02}m", secs / 3600, (secs % 3600) / 60),
    }
}

pub(crate) fn spinner(started: Instant) -> &'static str {
    const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    FRAMES[(started.elapsed().as_millis() / 80) as usize % FRAMES.len()]
}

pub(crate) fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("{n} {word}")
    } else {
        format!("{n} {word}s")
    }
}

pub(crate) fn truncate(name: &str, max: usize) -> String {
    if name.chars().count() <= max {
        return name.to_string();
    }
    let head: String = name.chars().take(max.saturating_sub(1)).collect();
    format!("{head}…")
}
