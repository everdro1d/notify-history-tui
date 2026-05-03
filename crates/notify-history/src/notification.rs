use chrono::{Local, TimeZone};

const DELIMITER: &str = "[*]";

// ── Notification ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Notification {
    pub timestamp: i64,
    pub app_name: String,
    pub summary: String,
    /// Body text with real newlines (unescaped on read)
    pub body: String,
}

impl Notification {
    /// Deserialise one line from the history file.
    pub fn from_line(line: &str) -> Option<Self> {
        let parts: Vec<&str> = line.splitn(4, DELIMITER).collect();
        if parts.len() < 4 {
            return None;
        }
        // Timestamps written by older daemon versions are whole seconds (~10
        // digits).  Newer entries are milliseconds (~13 digits).  Normalise
        // everything to milliseconds so datetime_str can show sub-second
        // precision.
        let raw: i64 = parts[0].trim().parse().ok()?;
        let timestamp = if raw < 100_000_000_000 { raw * 1000 } else { raw };
        Some(Self {
            timestamp,
            app_name: unescape_value(parts[1]),
            summary: unescape_value(parts[2]),
            body: unescape_value(parts[3]),
        })
    }

    /// Serialise to a single file line.
    pub fn to_line(&self) -> String {
        format!(
            "{}{}{}{}{}{}{}",
            self.timestamp,
            DELIMITER,
            escape_value(&self.app_name),
            DELIMITER,
            escape_value(&self.summary),
            DELIMITER,
            escape_value(&self.body),
        )
    }

    /// Combined string used for fuzzy-search scoring / filtering.
    pub fn search_text(&self) -> String {
        format!("{} {} {}", self.summary, self.app_name, self.body)
    }

    /// Human-readable timestamp string, choosing the longest format that fits
    /// within `max_cols` display columns.  Formats are tried longest-first,
    /// progressively shedding year, fractional seconds, month, then day.
    /// `HH:MM:SS` (8 cols) is the minimum; it is returned even if `max_cols < 8`.
    pub fn datetime_str_for_width(&self, max_cols: usize) -> String {
        match Local.timestamp_millis_opt(self.timestamp) {
            chrono::LocalResult::Single(dt) => {
                // (format_string, minimum_columns_needed)
                const FORMATS: &[(&str, usize)] = &[
                    ("%Y-%m-%d %H:%M:%S%.3f", 23),  // 2024-01-15 14:30:45.123
                    ("%m-%d %H:%M:%S%.3f", 18),     // 01-15 14:30:45.123
                    ("%m-%d %H:%M:%S", 14),         // 01-15 14:30:45
                    ("%d %H:%M:%S", 11),            // 15 14:30:45
                    ("%H:%M:%S", 8),                // 14:30:45
                ];
                for &(fmt, min_cols) in FORMATS {
                    if max_cols >= min_cols {
                        return dt.format(fmt).to_string();
                    }
                }
                // Fallback: return minimum even if it does not technically fit.
                dt.format("%H:%M:%S").to_string()
            }
            _ => format!("ts:{}", self.timestamp),
        }
    }

    /// Return the body text formatted for display.
    ///
    /// When `escape` is `false` (the default), literal two-character escape
    /// sequences such as `\n` and `\t` that appear in the stored body are
    /// expanded to real control characters so they render as formatting in the
    /// TUI.  When `escape` is `true` the body is returned unchanged, so those
    /// sequences are visible as-is.
    pub fn display_body(&self, escape: bool) -> String {
        if escape {
            self.body.clone()
        } else {
            self.body
                .replace("\\n", "\n")
                .replace("\\t", "\t")
        }
    }
}

// ── Escape / unescape ─────────────────────────────────────────────────────────

pub fn escape_value(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x08' => out.push_str("\\b"),
            '\x0C' => out.push_str("\\f"),
            c => out.push(c),
        }
    }
    out
}

pub fn unescape_value(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some('b') => out.push('\x08'),
                Some('f') => out.push('\x0C'),
                Some(c) => {
                    out.push('\\');
                    out.push(c);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}
