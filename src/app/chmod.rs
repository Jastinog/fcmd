use super::*;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

pub struct ChownPicker {
    pub users: Vec<(String, u32)>,
    pub groups: Vec<(String, u32)>,
    pub user_cursor: usize,
    pub group_cursor: usize,
    pub user_scroll: usize,
    pub group_scroll: usize,
    pub column: usize, // 0 = user, 1 = group
    pub paths: Vec<PathBuf>,
    pub current_uid: Option<u32>,
    pub current_gid: Option<u32>,
}

impl ChownPicker {
    /// Effective visible rows per column (matches render_chown_picker layout).
    /// popup_h = min(20, visible_height+2).max(10), inner = popup_h-2,
    /// list_area = inner-2 (separator+hint), per-column = list_area-1 (header)
    pub fn list_height(visible_height: usize) -> usize {
        let popup_h = 20usize.min(visible_height + 2).max(10);
        // 2 border + 1 separator + 1 hint + 1 header = 5
        popup_h.saturating_sub(5).max(1)
    }
}

impl App {
    pub(super) fn enter_chmod(&mut self) {
        let paths = self.active_panel().targeted_paths();
        if paths.is_empty() {
            return;
        }
        let prefill = read_octal_mode(&paths[0])
            .map(|m| format!("{m:o}"))
            .unwrap_or_default();
        self.rename_input = prefill;
        self.chmod_paths = paths;
        self.mode = Mode::Chmod;
    }

    pub(super) fn enter_chown(&mut self) {
        let paths = self.active_panel().targeted_paths();
        if paths.is_empty() {
            return;
        }

        let (current_uid, current_gid) = read_uid_gid(&paths[0]).unwrap_or((0, 0));

        let mut users = list_system_users();
        let mut groups = list_system_groups();

        // Sort: regular users first (no _ prefix), then system users
        users.sort_by(|a, b| {
            let a_sys = a.0.starts_with('_');
            let b_sys = b.0.starts_with('_');
            a_sys.cmp(&b_sys).then(a.0.cmp(&b.0))
        });
        groups.sort_by(|a, b| {
            let a_sys = a.0.starts_with('_');
            let b_sys = b.0.starts_with('_');
            a_sys.cmp(&b_sys).then(a.0.cmp(&b.0))
        });

        let user_cursor = users
            .iter()
            .position(|(_, uid)| *uid == current_uid)
            .unwrap_or(0);
        let group_cursor = groups
            .iter()
            .position(|(_, gid)| *gid == current_gid)
            .unwrap_or(0);

        let list_h = ChownPicker::list_height(self.visible_height);
        let user_scroll = user_cursor.saturating_sub(list_h / 2);
        let group_scroll = group_cursor.saturating_sub(list_h / 2);

        self.chown_picker = Some(ChownPicker {
            users,
            groups,
            user_cursor,
            group_cursor,
            user_scroll,
            group_scroll,
            column: 0,
            paths,
            current_uid: Some(current_uid),
            current_gid: Some(current_gid),
        });
        self.mode = Mode::Chown;
    }

    pub(super) fn handle_chmod(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                let input = self.rename_input.trim().to_string();
                if input.is_empty() {
                    self.mode = Mode::Normal;
                    self.chmod_paths.clear();
                    return;
                }
                // Validate: 3-4 octal digits (0-7 only)
                if !(3..=4).contains(&input.len())
                    || !input.chars().all(|c| c.is_ascii_digit() && c <= '7')
                {
                    self.status_message = "Invalid octal mode (e.g. 755)".into();
                    return;
                }
                let mode = match u32::from_str_radix(&input, 8) {
                    Ok(m) => m,
                    Err(_) => {
                        self.status_message = "Invalid octal mode".into();
                        return;
                    }
                };
                let paths = std::mem::take(&mut self.chmod_paths);
                let n = paths.len();
                let mut errors = 0;
                for p in &paths {
                    if let Err(e) = ops::chmod(p, mode) {
                        self.status_message = format!("chmod: {e}");
                        errors += 1;
                    }
                }
                if errors == 0 {
                    self.status_message = format!("chmod {input} ({n} item(s))");
                }
                self.refresh_panels();
                self.mode = Mode::Normal;
            }
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.chmod_paths.clear();
            }
            KeyCode::Backspace => {
                if self.rename_input.is_empty() {
                    self.mode = Mode::Normal;
                    self.chmod_paths.clear();
                } else {
                    self.rename_input.pop();
                }
            }
            KeyCode::Char(c) if c.is_ascii_digit() && c <= '7' => {
                if self.rename_input.len() < 4 {
                    self.rename_input.push(c);
                }
            }
            _ => {}
        }
    }

    pub(super) fn handle_chown(&mut self, key: KeyEvent) {
        let Some(ref mut picker) = self.chown_picker else {
            self.mode = Mode::Normal;
            return;
        };

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if picker.column == 0 {
                    if picker.user_cursor + 1 < picker.users.len() {
                        picker.user_cursor += 1;
                    }
                } else if picker.group_cursor + 1 < picker.groups.len() {
                    picker.group_cursor += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if picker.column == 0 {
                    picker.user_cursor = picker.user_cursor.saturating_sub(1);
                } else {
                    picker.group_cursor = picker.group_cursor.saturating_sub(1);
                }
            }
            KeyCode::Char('G') => {
                if picker.column == 0 {
                    picker.user_cursor = picker.users.len().saturating_sub(1);
                } else {
                    picker.group_cursor = picker.groups.len().saturating_sub(1);
                }
            }
            KeyCode::Char('g') => {
                if picker.column == 0 {
                    picker.user_cursor = 0;
                } else {
                    picker.group_cursor = 0;
                }
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = ChownPicker::list_height(self.visible_height) / 2;
                if picker.column == 0 {
                    picker.user_cursor =
                        (picker.user_cursor + half).min(picker.users.len().saturating_sub(1));
                } else {
                    picker.group_cursor =
                        (picker.group_cursor + half).min(picker.groups.len().saturating_sub(1));
                }
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = ChownPicker::list_height(self.visible_height) / 2;
                if picker.column == 0 {
                    picker.user_cursor = picker.user_cursor.saturating_sub(half);
                } else {
                    picker.group_cursor = picker.group_cursor.saturating_sub(half);
                }
            }
            KeyCode::Tab | KeyCode::Char('l') | KeyCode::Right => {
                picker.column = if picker.column == 0 { 1 } else { 0 };
            }
            KeyCode::Char('h') | KeyCode::Left => {
                picker.column = if picker.column == 0 { 1 } else { 0 };
            }
            KeyCode::Enter => {
                let uid = picker.users.get(picker.user_cursor).map(|(_, id)| *id);
                let gid = picker.groups.get(picker.group_cursor).map(|(_, id)| *id);
                let user_name = picker
                    .users
                    .get(picker.user_cursor)
                    .map(|(n, _)| n.clone())
                    .unwrap_or_default();
                let group_name = picker
                    .groups
                    .get(picker.group_cursor)
                    .map(|(n, _)| n.clone())
                    .unwrap_or_default();
                let paths = std::mem::take(&mut picker.paths);
                let n = paths.len();
                self.chown_picker = None;
                let mut errors = 0;
                for p in &paths {
                    if let Err(e) = ops::chown(p, uid, gid) {
                        self.status_message = format!("chown: {e}");
                        errors += 1;
                    }
                }
                if errors == 0 {
                    self.status_message =
                        format!("chown {user_name}:{group_name} ({n} item(s))");
                }
                self.refresh_panels();
                self.mode = Mode::Normal;
            }
            KeyCode::Esc => {
                self.chown_picker = None;
                self.mode = Mode::Normal;
            }
            _ => {}
        }

        // Adjust scroll to follow cursor
        if let Some(ref mut picker) = self.chown_picker {
            let list_h = ChownPicker::list_height(self.visible_height);
            adjust_scroll(&mut picker.user_scroll, picker.user_cursor, list_h);
            adjust_scroll(&mut picker.group_scroll, picker.group_cursor, list_h);
        }
    }
}

fn adjust_scroll(scroll: &mut usize, cursor: usize, visible: usize) {
    if cursor < *scroll {
        *scroll = cursor;
    } else if cursor >= *scroll + visible {
        *scroll = cursor - visible + 1;
    }
}

#[cfg(unix)]
fn read_octal_mode(path: &std::path::Path) -> Option<u32> {
    let meta = std::fs::metadata(path).ok()?;
    Some(meta.permissions().mode() & 0o7777)
}

#[cfg(unix)]
fn read_uid_gid(path: &std::path::Path) -> Option<(u32, u32)> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.uid(), meta.gid()))
}

#[cfg(unix)]
fn list_system_users() -> Vec<(String, u32)> {
    let mut seen = HashSet::new();
    let mut users = Vec::new();
    unsafe {
        libc::setpwent();
        loop {
            let pw = libc::getpwent();
            if pw.is_null() {
                break;
            }
            let uid = (*pw).pw_uid;
            if seen.insert(uid) {
                let name = std::ffi::CStr::from_ptr((*pw).pw_name)
                    .to_string_lossy()
                    .into_owned();
                users.push((name, uid));
            }
        }
        libc::endpwent();
    }
    users
}

#[cfg(unix)]
fn list_system_groups() -> Vec<(String, u32)> {
    let mut seen = HashSet::new();
    let mut groups = Vec::new();
    unsafe {
        libc::setgrent();
        loop {
            let gr = libc::getgrent();
            if gr.is_null() {
                break;
            }
            let gid = (*gr).gr_gid;
            if seen.insert(gid) {
                let name = std::ffi::CStr::from_ptr((*gr).gr_name)
                    .to_string_lossy()
                    .into_owned();
                groups.push((name, gid));
            }
        }
        libc::endgrent();
    }
    groups
}

pub(crate) fn format_rwx(mode: u32) -> String {
    let mut s = String::with_capacity(9);
    let flags = [
        (0o400, 'r'),
        (0o200, 'w'),
        (0o100, 'x'),
        (0o040, 'r'),
        (0o020, 'w'),
        (0o010, 'x'),
        (0o004, 'r'),
        (0o002, 'w'),
        (0o001, 'x'),
    ];
    for (bit, ch) in flags {
        s.push(if mode & bit != 0 { ch } else { '-' });
    }
    s
}
