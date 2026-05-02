use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use crate::config::Config;
use crate::filter::Filter;
use crate::notification::Notification;

// ── Mode ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Normal,
    Filter,
    ConfirmClearAll,
    ConfirmClearSelected,
    Help,
}

// ── Per-notification match indices for highlight rendering ────────────────────

#[derive(Default)]
pub struct NotifMatchIndices {
    pub summary: Vec<usize>,
    pub app_name: Vec<usize>,
    /// Match indices per displayed body line (indexed by line position in the body)
    pub body_per_line: Vec<Vec<usize>>,
}

// ── App ───────────────────────────────────────────────────────────────────────

pub struct App {
    /// All notifications in newest-first order (as stored in the file).
    pub all_notifications: Vec<Notification>,
    /// Indices into `all_notifications` that pass the current filter.
    pub display_list: Vec<usize>,

    /// Cursor position within the *current page* (0-based).
    pub cursor: usize,
    /// Current page (0-based).
    pub page: usize,
    /// How many notifications fit on one page (recalculated each frame).
    pub items_per_page: usize,

    /// Indices into `all_notifications` that are multi-selected.
    pub multi_selected: HashSet<usize>,

    pub mode: Mode,
    pub filter_input: String,

    pub config: Config,
    pub history_file: PathBuf,

    /// Rows a single notification occupies on screen (title + body + datetime + separator).
    pub rows_per_notif: usize,

    filter: Filter,
}

impl App {
    pub fn new(config: Config) -> Self {
        let history_file = config.history_file();
        let rows_per_notif = 3 + config.display.body_lines as usize;
        let mut app = Self {
            all_notifications: Vec::new(),
            display_list: Vec::new(),
            cursor: 0,
            page: 0,
            items_per_page: 1,
            multi_selected: HashSet::new(),
            mode: Mode::Normal,
            filter_input: String::new(),
            history_file,
            config,
            rows_per_notif,
            filter: Filter::new(),
        };
        app.load_notifications();
        app
    }

    // ── I/O ───────────────────────────────────────────────────────────────────

    /// Re-read the history file and rebuild the display list.
    pub fn load_notifications(&mut self) {
        self.all_notifications = if self.history_file.exists() {
            fs::read_to_string(&self.history_file)
                .unwrap_or_default()
                .lines()
                .filter(|l| !l.trim().is_empty())
                .filter_map(Notification::from_line)
                .collect()
        } else {
            Vec::new()
        };
        self.update_display_list();
    }

    fn save_notifications(&self) {
        if let Some(parent) = self.history_file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let mut content = self
            .all_notifications
            .iter()
            .map(|n| n.to_line())
            .collect::<Vec<_>>()
            .join("\n");
        if !content.is_empty() {
            content.push('\n');
        }
        let _ = fs::write(&self.history_file, content);
    }

    // ── Filtering ─────────────────────────────────────────────────────────────

    pub fn update_display_list(&mut self) {
        if self.filter_input.is_empty() {
            self.display_list = (0..self.all_notifications.len()).collect();
        } else {
            let query = self.filter_input.clone();
            self.display_list = (0..self.all_notifications.len())
                .filter(|&i| {
                    self.filter
                        .score(&query, &self.all_notifications[i].search_text())
                        .is_some()
                })
                .collect();
        }
        self.clamp_cursor();
    }

    // ── Layout ────────────────────────────────────────────────────────────────

    pub fn update_items_per_page(&mut self, content_height: u16) {
        let rows = self.rows_per_notif.max(1);
        // For N items each `item_rows` tall with N-1 separators, total height is
        // N*(item_rows+1)-1.  Solving gives N = (H+1)/(item_rows+1) where
        // rows_per_notif already includes the separator, so the formula is simply
        // (content_height + 1) / rows_per_notif.
        let new_count = ((content_height as usize + 1) / rows).max(1);
        if new_count != self.items_per_page {
            self.items_per_page = new_count;
            self.clamp_cursor();
        }
    }

    // ── Pagination helpers ────────────────────────────────────────────────────

    pub fn num_pages(&self) -> usize {
        if self.display_list.is_empty() {
            1
        } else {
            self.display_list.len().div_ceil(self.items_per_page)
        }
    }

    pub fn items_on_current_page(&self) -> usize {
        if self.display_list.is_empty() {
            return 0;
        }
        let start = self.page * self.items_per_page;
        let end = ((self.page + 1) * self.items_per_page).min(self.display_list.len());
        end.saturating_sub(start)
    }

    /// Global index into `display_list` for the cursor position.
    pub fn cursor_display_idx(&self) -> usize {
        self.page * self.items_per_page + self.cursor
    }

    /// Index into `all_notifications` that the cursor currently points at.
    pub fn cursor_notif_idx(&self) -> Option<usize> {
        self.display_list.get(self.cursor_display_idx()).copied()
    }

    fn clamp_cursor(&mut self) {
        if self.display_list.is_empty() {
            self.cursor = 0;
            self.page = 0;
            return;
        }
        let max_page = self.num_pages().saturating_sub(1);
        if self.page > max_page {
            self.page = max_page;
        }
        let max_cursor = self.items_on_current_page().saturating_sub(1);
        if self.cursor > max_cursor {
            self.cursor = max_cursor;
        }
    }

    // ── Navigation ────────────────────────────────────────────────────────────

    pub fn move_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        } else if self.page > 0 {
            self.page -= 1;
            self.cursor = self.items_on_current_page().saturating_sub(1);
        }
    }

    pub fn move_down(&mut self) {
        let items = self.items_on_current_page();
        if self.cursor + 1 < items {
            self.cursor += 1;
        } else if self.page + 1 < self.num_pages() {
            self.page += 1;
            self.cursor = 0;
        }
    }

    pub fn prev_page(&mut self) {
        if self.page > 0 {
            self.page -= 1;
            self.cursor = 0;
        }
    }

    pub fn next_page(&mut self) {
        if self.page + 1 < self.num_pages() {
            self.page += 1;
            self.cursor = 0;
        }
    }

    pub fn go_to_start(&mut self) {
        self.page = 0;
        self.cursor = 0;
    }

    pub fn go_to_end(&mut self) {
        self.page = self.num_pages().saturating_sub(1);
        self.cursor = self.items_on_current_page().saturating_sub(1);
    }

    // ── Selection ─────────────────────────────────────────────────────────────

    pub fn toggle_select_current(&mut self) {
        if let Some(idx) = self.cursor_notif_idx() {
            if !self.multi_selected.remove(&idx) {
                self.multi_selected.insert(idx);
            }
        }
    }

    // ── Mutations ─────────────────────────────────────────────────────────────

    pub fn remove_current_notification(&mut self) {
        if let Some(notif_idx) = self.cursor_notif_idx() {
            self.all_notifications.remove(notif_idx);
            // Shift multi-selected indices down past the removed entry
            let updated: HashSet<usize> = self
                .multi_selected
                .iter()
                .filter(|&&i| i != notif_idx)
                .map(|&i| if i > notif_idx { i - 1 } else { i })
                .collect();
            self.multi_selected = updated;
            self.save_notifications();
            self.update_display_list();
        }
    }

    pub fn clear_all_notifications(&mut self) {
        self.all_notifications.clear();
        self.multi_selected.clear();
        self.save_notifications();
        self.update_display_list();
        self.page = 0;
        self.cursor = 0;
    }

    pub fn clear_selected_notifications(&mut self) {
        // Remove in descending order so earlier indices stay valid
        let mut to_remove: Vec<usize> = self.multi_selected.iter().copied().collect();
        to_remove.sort_unstable_by(|a, b| b.cmp(a));
        for idx in to_remove {
            self.all_notifications.remove(idx);
        }
        self.multi_selected.clear();
        self.save_notifications();
        self.update_display_list();
    }

    // ── Fuzzy-match highlight indices ─────────────────────────────────────────

    pub fn get_match_indices(&self, notif_idx: usize) -> NotifMatchIndices {
        if self.filter_input.is_empty() {
            return NotifMatchIndices::default();
        }
        let notif = &self.all_notifications[notif_idx];
        let q = &self.filter_input;
        let body_lines: Vec<Vec<usize>> = notif
            .body
            .lines()
            .take(self.config.display.body_lines as usize)
            .map(|line| self.filter.match_indices(q, line).unwrap_or_default())
            .collect();

        NotifMatchIndices {
            summary: self.filter.match_indices(q, &notif.summary).unwrap_or_default(),
            app_name: self.filter.match_indices(q, &notif.app_name).unwrap_or_default(),
            body_per_line: body_lines,
        }
    }
}
