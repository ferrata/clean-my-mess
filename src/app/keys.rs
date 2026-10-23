use std::path::PathBuf;

use ratatui::crossterm::event::{KeyCode, KeyModifiers};

use super::{App, Choice, Mode};

impl App {
    pub(crate) fn on_key(&mut self, code: KeyCode, mods: KeyModifiers) {
        if mods.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        match self.mode {
            Mode::Confirm => self.on_key_confirm(code),
            Mode::Deleting => self.on_key_deleting(code),
            Mode::Normal => self.on_key_normal(code),
        }
    }

    fn on_key_confirm(&mut self, code: KeyCode) {
        match code {
            KeyCode::Right | KeyCode::Tab | KeyCode::Char('l') => self.choice = self.choice.next(),
            KeyCode::Left | KeyCode::BackTab | KeyCode::Char('h') => self.choice = self.choice.prev(),
            KeyCode::Char('t') => self.confirm_delete(false),
            KeyCode::Char('y') => self.confirm_delete(true),
            KeyCode::Enter | KeyCode::Char(' ') => match self.choice {
                Choice::Trash => self.confirm_delete(false),
                Choice::Delete => self.confirm_delete(true),
                Choice::Cancel => self.cancel_delete(),
            },
            KeyCode::Char('n' | 'q') | KeyCode::Esc => self.cancel_delete(),
            _ => {}
        }
    }

    fn on_key_deleting(&mut self, code: KeyCode) {
        if matches!(code, KeyCode::Esc | KeyCode::Char('q' | 's')) {
            self.stop_delete();
        }
    }

    fn on_key_normal(&mut self, code: KeyCode) {
        self.status.clear();
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Down | KeyCode::Char('j') => self.move_cursor(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_cursor(-1),
            KeyCode::PageDown => self.move_cursor(10),
            KeyCode::PageUp => self.move_cursor(-10),
            KeyCode::Home | KeyCode::Char('g') => self.cursor = 0,
            KeyCode::End | KeyCode::Char('G') => self.cursor = self.entries.len().saturating_sub(1),
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Enter => self.enter(),
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Backspace => self.up(),
            KeyCode::Char(' ') => self.toggle_mark(),
            KeyCode::Char('a') => self.mark_all(),
            KeyCode::Char('n') => self.selected.clear(),
            KeyCode::Char('d') | KeyCode::Char('x') | KeyCode::Delete => self.open_confirm(),
            _ => {}
        }
    }

    fn toggle_mark(&mut self) {
        if let Some(entry) = self.entries.get(self.cursor) {
            if !self.selected.remove(&entry.path) {
                self.selected.insert(entry.path.clone());
            }
            self.move_cursor(1);
        }
    }

    fn mark_all(&mut self) {
        let visible: Vec<PathBuf> = self.entries.iter().filter(|e| e.count > 0).map(|e| e.path.clone()).collect();
        self.selected.extend(visible);
    }

    fn open_confirm(&mut self) {
        if !self.targets().is_empty() {
            self.choice = Choice::Cancel;
            self.mode = Mode::Confirm;
        } else if self.entries.get(self.cursor).is_some() {
            self.status = "No waste found there — nothing to delete".into();
        }
    }
}
