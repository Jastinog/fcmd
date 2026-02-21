use super::*;

impl App {
    pub(super) fn handle_preview(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.file_preview = None;
                self.file_preview_path = None;
                self.preview_search_query.clear();
                self.preview_search_matches.clear();
                self.preview_search_current = 0;
                self.mode = Mode::Normal;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(ref mut p) = self.file_preview {
                    p.scroll_down(1, self.visible_height);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some(ref mut p) = self.file_preview {
                    p.scroll_up(1);
                }
            }
            KeyCode::Char('d') if ctrl => {
                if let Some(ref mut p) = self.file_preview {
                    let half = self.visible_height / 2;
                    p.scroll_down(half, self.visible_height);
                }
            }
            KeyCode::Char('u') if ctrl => {
                if let Some(ref mut p) = self.file_preview {
                    let half = self.visible_height / 2;
                    p.scroll_up(half);
                }
            }
            KeyCode::Char('f') if ctrl => {
                if let Some(ref mut p) = self.file_preview {
                    p.scroll_down(self.visible_height, self.visible_height);
                }
            }
            KeyCode::Char('b') if ctrl => {
                if let Some(ref mut p) = self.file_preview {
                    p.scroll_up(self.visible_height);
                }
            }
            KeyCode::Char('G') => {
                if let Some(ref mut p) = self.file_preview {
                    let max = p.lines.len().saturating_sub(self.visible_height);
                    p.scroll = max;
                    p.hscroll = 0;
                }
            }
            KeyCode::Char('g') => {
                if let Some(ref mut p) = self.file_preview {
                    p.scroll = 0;
                    p.hscroll = 0;
                }
            }
            KeyCode::Char('h') | KeyCode::Left => {
                if let Some(ref mut p) = self.file_preview {
                    p.scroll_left(1);
                }
            }
            KeyCode::Char('l') | KeyCode::Right => {
                if let Some(ref mut p) = self.file_preview {
                    p.scroll_right(1);
                }
            }
            KeyCode::Char('H') => {
                if let Some(ref mut p) = self.file_preview {
                    p.scroll_left(8);
                }
            }
            KeyCode::Char('L') => {
                if let Some(ref mut p) = self.file_preview {
                    p.scroll_right(8);
                }
            }
            KeyCode::Char('0') => {
                if let Some(ref mut p) = self.file_preview {
                    p.hscroll = 0;
                }
            }
            KeyCode::Char('$') => {
                if let Some(ref mut p) = self.file_preview {
                    let visible = self.visible_height;
                    let max_line_width = p.lines.iter()
                        .skip(p.scroll)
                        .take(visible)
                        .map(|l| unicode_width::UnicodeWidthStr::width(l.as_str()))
                        .max()
                        .unwrap_or(0);
                    p.hscroll = max_line_width.saturating_sub(10);
                }
            }
            KeyCode::Char('o') => {
                if let Some(path) = self.file_preview_path.clone() {
                    self.request_open_editor(path);
                }
            }
            KeyCode::Char('/') => {
                self.preview_search_query.clear();
                self.preview_search_matches.clear();
                self.preview_search_current = 0;
                self.mode = Mode::PreviewSearch;
            }
            KeyCode::Char('n') => {
                if !self.preview_search_matches.is_empty() {
                    self.preview_search_current =
                        (self.preview_search_current + 1) % self.preview_search_matches.len();
                    self.scroll_preview_to_match();
                }
            }
            KeyCode::Char('N') => {
                if !self.preview_search_matches.is_empty() {
                    let len = self.preview_search_matches.len();
                    self.preview_search_current =
                        (self.preview_search_current + len - 1) % len;
                    self.scroll_preview_to_match();
                }
            }
            _ => {}
        }
    }

    pub(super) fn handle_preview_search(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(c) => {
                self.preview_search_query.push(c);
                self.update_preview_search_matches();
            }
            KeyCode::Backspace => {
                self.preview_search_query.pop();
                self.update_preview_search_matches();
            }
            KeyCode::Enter => {
                self.mode = Mode::Preview;
            }
            KeyCode::Esc => {
                self.preview_search_query.clear();
                self.preview_search_matches.clear();
                self.preview_search_current = 0;
                self.mode = Mode::Preview;
            }
            _ => {}
        }
    }

    fn update_preview_search_matches(&mut self) {
        self.preview_search_matches.clear();
        self.preview_search_current = 0;
        let query = self.preview_search_query.to_lowercase();
        if query.is_empty() {
            return;
        }
        if let Some(ref p) = self.file_preview {
            for (line_idx, line) in p.lines.iter().enumerate() {
                let line_lower = line.to_lowercase();
                let mut start = 0;
                while let Some(pos) = line_lower[start..].find(&query) {
                    self.preview_search_matches.push((line_idx, start + pos));
                    start += pos + query.len();
                }
            }
        }
        if !self.preview_search_matches.is_empty() {
            self.scroll_preview_to_match();
        }
    }

    fn scroll_preview_to_match(&mut self) {
        if let Some(&(line_idx, _)) = self.preview_search_matches.get(self.preview_search_current)
            && let Some(ref mut p) = self.file_preview
        {
            let visible = self.visible_height;
            if line_idx < p.scroll || line_idx >= p.scroll + visible {
                p.scroll = line_idx.saturating_sub(visible / 3);
            }
        }
    }
}
