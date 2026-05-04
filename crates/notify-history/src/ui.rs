use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Mode};
use crate::config::ColorConfig;

// ── Colour helpers ────────────────────────────────────────────────────────────

struct AppColors {
    fg: Color,
    bg: Color,
    accent: Color,
    highlight: Color,
    matching: Color,
}

fn parse_hex(s: &str) -> Color {
    let s = s.trim_start_matches('#');
    if s.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&s[0..2], 16),
            u8::from_str_radix(&s[2..4], 16),
            u8::from_str_radix(&s[4..6], 16),
        ) {
            return Color::Rgb(r, g, b);
        }
    }
    Color::Reset
}

fn parse_colors(cfg: &ColorConfig) -> AppColors {
    AppColors {
        fg: parse_hex(&cfg.foreground),
        bg: parse_hex(&cfg.background),
        accent: parse_hex(&cfg.accent),
        highlight: parse_hex(&cfg.highlight),
        matching: parse_hex(&cfg.matching),
    }
}

// ── Main draw entry point ─────────────────────────────────────────────────────

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let colors = parse_colors(&app.config.colors);

    // Paint background
    f.render_widget(
        Block::default().style(Style::default().bg(colors.bg)),
        area,
    );

    let in_filter = matches!(app.mode, Mode::Filter);
    let hints_visible = app.show_hints;

    // Build layout constraints (header | count | blank | content | [filter] | dots | [hints])
    let mut constraints: Vec<Constraint> = vec![
        Constraint::Length(1), // header
        Constraint::Length(1), // count bar
        Constraint::Length(1), // blank gap below count bar
        Constraint::Fill(1),   // notification list
    ];
    if in_filter {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Length(1)); // dots row (blank when 1 page)
    if hints_visible {
        constraints.push(Constraint::Length(1)); // hints bar
    }

    let chunks = Layout::vertical(constraints).split(area);
    let header_area = chunks[0];
    let count_area = chunks[1];
    let content_area = chunks[3];

    // Must happen before rendering so pagination is correct
    app.update_items_per_page(content_area.height);

    let mut chunk_idx: usize = 4;

    if in_filter {
        render_filter_bar(f, app, chunks[chunk_idx], &colors);
        chunk_idx += 1;
    }

    let dots_area = chunks[chunk_idx];
    chunk_idx += 1;

    render_header(f, app, header_area, &colors);
    render_count_bar(f, app, count_area, &colors);
    render_notifications(f, app, content_area, &colors);
    render_dots(f, app, dots_area, &colors, !hints_visible);
    if hints_visible {
        render_hints(f, app, chunks[chunk_idx], &colors);
    }

    // Modal overlays – clone mode first so the borrow of app.mode is released
    // before we pass `app` mutably into render_help_popup.
    let mode = app.mode.clone();
    match mode {
        Mode::ConfirmClearAll => {
            render_confirm_dialog(f, area, &colors, "Clear ALL notifications?");
        }
        Mode::ConfirmClearSelected => {
            render_confirm_dialog(f, area, &colors, "Clear SELECTED notifications?");
        }
        Mode::Help => render_help_popup(f, app, area, &colors),
        _ => {}
    }
}

// ── Header ────────────────────────────────────────────────────────────────────

fn render_header(f: &mut Frame, _app: &App, area: Rect, colors: &AppColors) {
    let title = if area.width >= 21 {
        " Notification History"
    } else if area.width >= 14 {
        " Notifications"
    } else {
        " Notifs."
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            title.to_owned(),
            Style::default().fg(colors.accent).bg(colors.bg),
        )))
        .style(Style::default().bg(colors.bg)),
        area,
    );
}

// ── Count bar ─────────────────────────────────────────────────────────────────

fn render_count_bar(f: &mut Frame, app: &App, area: Rect, colors: &AppColors) {
    let line = if area.width >= 21 {
        "─── "
    } else {
        ""
    };
    let total = app.all_notifications.len();
    let label = if app.filter_input.is_empty() {
        format!(" {}{} items {}", line, total, line)
    } else {
        format!(" {}{} of {} {}", line, app.display_list.len(), total, line)
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            label,
            Style::default()
                .fg(colors.fg)
                .bg(colors.bg)
                .add_modifier(Modifier::DIM),
        )))
        .style(Style::default().bg(colors.bg)),
        area,
    );
}

// ── Text layout helpers ───────────────────────────────────────────────────────

/// Truncate `text` to at most `max_cols` display columns (char-level).
fn truncate_to_cols(text: &str, max_cols: usize) -> String {
    let mut out = String::new();
    let mut width: usize = 0;
    for ch in text.chars() {
        let ch_w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + ch_w > max_cols {
            break;
        }
        out.push(ch);
        width += ch_w;
    }
    out
}

/// Wrap `text` into sub-lines that each fit within `max_cols` display columns.
/// Returns a list of `(sub_line_text, char_start_offset_in_source)`.
/// An empty `text` produces a single entry `("", 0)`.
fn wrap_to_cols(text: &str, max_cols: usize) -> Vec<(String, usize)> {
    if max_cols == 0 {
        return vec![(String::new(), 0)];
    }
    let mut result: Vec<(String, usize)> = Vec::new();
    let mut current = String::new();
    let mut current_width: usize = 0;
    let mut current_char_start: usize = 0;

    for (char_idx, ch) in text.chars().enumerate() {
        let ch_w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if current_width + ch_w > max_cols && !current.is_empty() {
            result.push((current.clone(), current_char_start));
            current.clear();
            current_width = 0;
            current_char_start = char_idx;
        }
        current.push(ch);
        current_width += ch_w;
    }
    // Always push the last (or only) sub-line, even when text is empty.
    if !current.is_empty() || result.is_empty() {
        result.push((current, current_char_start));
    }
    result
}

/// Word-wrap `text` into sub-lines that each fit within `max_cols` display columns.
/// Breaks only on whitespace boundaries. A single word that is wider than `max_cols`
/// is still placed on its own line (the caller is responsible for truncation).
/// An empty `text` produces a single entry `""`.
fn word_wrap_to_cols(text: &str, max_cols: usize) -> Vec<String> {
    if max_cols == 0 || text.is_empty() {
        return vec![text.to_string()];
    }
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_w: usize = 0;
    for word in text.split_whitespace() {
        let word_w = word.width();
        if current.is_empty() {
            current.push_str(word);
            current_w = word_w;
        } else if current_w + 1 + word_w <= max_cols {
            current.push(' ');
            current.push_str(word);
            current_w += 1 + word_w;
        } else {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
            current_w = word_w;
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

/// Append trailing spaces so the line's spans visually fill `width` columns.
fn pad_line_to_width(mut spans: Vec<Span<'static>>, width: u16, pad_style: Style) -> Vec<Span<'static>> {
    let current: usize = spans.iter().map(|s| s.content.width()).sum();
    let remaining = (width as usize).saturating_sub(current);
    if remaining > 0 {
        spans.push(Span::styled(" ".repeat(remaining), pad_style));
    }
    spans
}

fn render_notifications(f: &mut Frame, app: &App, area: Rect, colors: &AppColors) {
    let mut lines: Vec<Line<'static>> = Vec::new();

    let start = app.page * app.items_per_page;
    let end = (start + app.items_per_page).min(app.display_list.len());

    if start >= end {
        // Empty state
        let msg = if app.filter_input.is_empty() {
            "  No notifications."
        } else {
            "  No notifications match the filter."
        };
        lines.push(Line::from(Span::styled(
            msg.to_owned(),
            Style::default().fg(colors.fg).bg(colors.bg),
        )));
    } else {
        let body_lines_count = app.effective_body_lines;
        let show_app = app.config.display.show_app;
        let cursor_disp_idx = app.cursor_display_idx();
        let total = end - start;

        for (pos, disp_idx) in (start..end).enumerate() {
            let notif_idx = app.display_list[disp_idx];
            let notif = &app.all_notifications[notif_idx];
            let is_cursor = disp_idx == cursor_disp_idx;
            let is_selected = app.multi_selected.contains(&notif_idx);

            let item_bg = if is_cursor { colors.highlight } else { colors.bg };
            let normal = Style::default().fg(colors.fg).bg(item_bg);
            let accent = Style::default().fg(colors.accent).bg(item_bg);
            let dim = Style::default()
                .fg(colors.fg)
                .bg(item_bg)
                .add_modifier(Modifier::DIM);
            let match_style = Style::default().fg(colors.matching).bg(item_bg);

            let match_idx = app.get_match_indices(notif_idx);

            // ── Line 1: marker + summary [— app_name] ──────────────────────
            let marker = if is_selected {
                "● "
            } else if is_cursor {
                "│ "
            } else {
                "  "
            };

            // Available columns for text after the 2-col marker prefix.
            let title_available = (area.width as usize).saturating_sub(2);
            let summary_w = notif.summary.width();

            let mut spans: Vec<Span<'static>> = vec![Span::styled(marker.to_owned(), accent)];
            if show_app && !notif.app_name.is_empty() {
                let sep_w: usize = 3; // " — "
                let app_w = notif.app_name.width();
                if summary_w + sep_w + app_w <= title_available {
                    // Both summary and app name fit.
                    spans.extend(styled_with_matches(
                        &notif.summary,
                        &match_idx.summary,
                        normal,
                        match_style,
                    ));
                    spans.push(Span::styled(" — ".to_owned(), normal));
                    spans.extend(styled_with_matches(
                        &notif.app_name,
                        &match_idx.app_name,
                        accent,
                        match_style,
                    ));
                } else if summary_w <= title_available {
                    // Only summary fits (drop app name).
                    spans.extend(styled_with_matches(
                        &notif.summary,
                        &match_idx.summary,
                        normal,
                        match_style,
                    ));
                } else {
                    // Truncate summary with "...".
                    let truncated = truncate_to_cols(&notif.summary, title_available.saturating_sub(3));
                    let kept = truncated.chars().count();
                    let filtered: Vec<usize> = match_idx.summary.iter().copied().filter(|&i| i < kept).collect();
                    spans.extend(styled_with_matches(&format!("{truncated}..."), &filtered, normal, match_style));
                }
            } else {
                if summary_w <= title_available {
                    spans.extend(styled_with_matches(
                        &notif.summary,
                        &match_idx.summary,
                        normal,
                        match_style,
                    ));
                } else {
                    let truncated = truncate_to_cols(&notif.summary, title_available.saturating_sub(3));
                    let kept = truncated.chars().count();
                    let filtered: Vec<usize> = match_idx.summary.iter().copied().filter(|&i| i < kept).collect();
                    spans.extend(styled_with_matches(&format!("{truncated}..."), &filtered, normal, match_style));
                }
            }
            if is_cursor {
                spans = pad_line_to_width(spans, area.width, normal);
            }
            lines.push(Line::from(spans).style(normal));

            // ── Lines 2 … (1+body_lines): body ─────────────────────────────
            if body_lines_count > 0 {
                // Available cols for body text after the 3-col prefix ("│  " / "   ").
                let body_available = (area.width as usize).saturating_sub(3).max(1);
                let body_display = notif.display_body(app.config.display.escape_body);
                let source_lines: Vec<&str> = body_display.lines().collect();
                let empty: Vec<usize> = Vec::new();

                // Flatten all source lines into wrapped sub-lines, carrying
                // char-start offsets so match indices can be remapped.
                let mut all_sub_lines: Vec<(String, Vec<usize>)> = Vec::new();
                for (src_i, &src_line) in source_lines.iter().enumerate() {
                    let src_indices = match_idx.body_per_line.get(src_i).unwrap_or(&empty);
                    for (sub_text, char_start) in wrap_to_cols(src_line, body_available) {
                        let sub_len = sub_text.chars().count();
                        let remapped: Vec<usize> = src_indices
                            .iter()
                            .copied()
                            .filter(|&i| i >= char_start && i < char_start + sub_len)
                            .map(|i| i - char_start)
                            .collect();
                        all_sub_lines.push((sub_text, remapped));
                    }
                }

                let has_overflow = all_sub_lines.len() > body_lines_count;

                for slot in 0..body_lines_count {
                    let prefix = if is_cursor {
                        Span::styled("│  ".to_owned(), accent)
                    } else {
                        Span::styled("   ".to_owned(), normal)
                    };
                    let mut row: Vec<Span<'static>> = vec![prefix];

                    if let Some((sub_text, sub_indices)) = all_sub_lines.get(slot) {
                        let is_last_slot = slot + 1 == body_lines_count;
                        if is_last_slot && has_overflow {
                            // More content follows — truncate and append "...".
                            let truncated = truncate_to_cols(sub_text, body_available.saturating_sub(3));
                            let kept = truncated.chars().count();
                            let filtered: Vec<usize> = sub_indices.iter().copied().filter(|&i| i < kept).collect();
                            row.extend(styled_with_matches(&format!("{truncated}..."), &filtered, normal, match_style));
                        } else {
                            row.extend(styled_with_matches(sub_text, sub_indices, normal, match_style));
                        }
                    }
                    // Empty slot: row already contains just the prefix span.

                    if is_cursor {
                        row = pad_line_to_width(row, area.width, normal);
                    }
                    lines.push(Line::from(row).style(normal));
                }
            }

            // ── Last body line: datetime ────────────────────────────────────
            let dt_available = (area.width as usize).saturating_sub(3);
            let dt_prefix = if is_cursor {
                Span::styled("│  ".to_owned(), accent)
            } else {
                Span::styled("   ".to_owned(), dim)
            };
            let mut dt_row = vec![dt_prefix, Span::styled(notif.datetime_str_for_width(dt_available), dim)];
            if is_cursor {
                dt_row = pad_line_to_width(dt_row, area.width, normal);
            }
            lines.push(Line::from(dt_row).style(dim));

            // ── Separator (between items) ───────────────────────────────────
            if pos + 1 < total {
                let sep = "─".repeat(area.width as usize);
                lines.push(Line::from(Span::styled(
                    sep,
                    Style::default().fg(colors.highlight).bg(colors.bg),
                )));
            }
        }
    }

    f.render_widget(
        Paragraph::new(Text::from(lines)).style(Style::default().bg(colors.bg)),
        area,
    );
}

// ── Page dots ─────────────────────────────────────────────────────────────────

/// Minimum terminal width at which the dots row is rendered (1 dot + both markers).
const DOTS_MIN_WIDTH: usize = 11;
/// Column width of each truncation marker ("NN..." / "...NN" / "99+.." / "..+99").
const DOTS_MARKER_WIDTH: usize = 5;

fn render_dots(f: &mut Frame, app: &App, area: Rect, colors: &AppColors, show_help_hint: bool) {
    // When showing the [?] hint on this row, reserve 3 cols on the right for it.
    let help_hint_width: u16 = if show_help_hint { 3 } else { 0 };
    let dots_area = Rect::new(
        area.x,
        area.y,
        area.width.saturating_sub(help_hint_width),
        area.height,
    );

    // Always cap the dot budget as if [?] is present, so toggling the hint bar
    // never changes how many dots are shown.
    let constrained_width = area.width.saturating_sub(3) as usize;
    render_dots_content(f, app, dots_area, colors, constrained_width);

    // Render [?] right-aligned when the hints bar is hidden.
    if show_help_hint {
        let hint_area = Rect::new(
            area.x + dots_area.width,
            area.y,
            help_hint_width,
            area.height,
        );
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "[?]".to_owned(),
                Style::default().fg(colors.accent).bg(colors.bg),
            )))
            .style(Style::default().bg(colors.bg)),
            hint_area,
        );
    }
}

fn render_dots_content(f: &mut Frame, app: &App, area: Rect, colors: &AppColors, constrained_width: usize) {
    let n = app.num_pages();
    if n <= 1 {
        f.render_widget(
            Paragraph::new("").style(Style::default().bg(colors.bg)),
            area,
        );
        return;
    }

    // `budget` governs how many dots are shown (capped as if [?] is always present).
    // `render_width` is the actual draw area, used only for centering.
    let budget = constrained_width;
    let render_width = area.width as usize;
    // Each dot is 1 col, spaces between are 1 col → total = n * 2 - 1
    let full_width = n * 2 - 1;

    // ── All dots fit: render normally ────────────────────────────────────────
    if full_width <= budget {
        let mut dot_spans: Vec<Span<'static>> = Vec::new();
        for i in 0..n {
            let (ch, style) = if i == app.page {
                ("●", Style::default().fg(colors.accent).bg(colors.bg))
            } else {
                ("○", Style::default().fg(colors.fg).bg(colors.bg))
            };
            dot_spans.push(Span::styled(ch.to_owned(), style));
            if i + 1 < n {
                dot_spans.push(Span::styled(" ".to_owned(), Style::default().bg(colors.bg)));
            }
        }
        let pad = render_width.saturating_sub(full_width) / 2;
        let mut all: Vec<Span<'static>> =
            vec![Span::styled(" ".repeat(pad), Style::default().bg(colors.bg))];
        all.extend(dot_spans);
        f.render_widget(
            Paragraph::new(Line::from(all)).style(Style::default().bg(colors.bg)),
            area,
        );
        return;
    }

    // ── Below minimum width: render blank ────────────────────────────────────
    if budget < DOTS_MIN_WIDTH {
        f.render_widget(
            Paragraph::new("").style(Style::default().bg(colors.bg)),
            area,
        );
        return;
    }

    // ── Truncated rendering ──────────────────────────────────────────────────
    // Each marker is DOTS_MARKER_WIDTH cols when present (both = 2*DOTS_MARKER_WIDTH - 1 fixed chars).
    // Solve for k visible dots: (2*DOTS_MARKER_WIDTH - 1) + 2k ≤ budget  →  k = (budget - (2*DOTS_MARKER_WIDTH - 1)) / 2, min 1.
    // k is always ≥ 1 due to .max(1) and the DOTS_MIN_WIDTH guard above.
    let k = ((budget - (2 * DOTS_MARKER_WIDTH - 1)) / 2).max(1);

    // Centre the window on the current page, then clamp so it stays in bounds.
    let half = (k - 1) / 2;
    let ideal_start = app.page.saturating_sub(half);
    let window_start = ideal_start.min(n.saturating_sub(k));
    let window_end = window_start + k - 1;

    let left_count = window_start;           // pages hidden to the left
    let right_count = n - 1 - window_end;    // pages hidden to the right

    let dim_s = Style::default()
        .fg(colors.fg)
        .bg(colors.bg)
        .add_modifier(Modifier::DIM);

    let mut spans: Vec<Span<'static>> = Vec::new();

    // Left marker – omitted when nothing is hidden on that side.
    if left_count > 0 {
        let s = if left_count >= 99 {
            "99+..".to_owned()
        } else {
            format!("{:2}...", left_count)
        };
        spans.push(Span::styled(s, dim_s));
    }

    // Visible dots
    for i in window_start..=window_end {
        let (ch, style) = if i == app.page {
            ("●", Style::default().fg(colors.accent).bg(colors.bg))
        } else {
            ("○", Style::default().fg(colors.fg).bg(colors.bg))
        };
        spans.push(Span::styled(ch.to_owned(), style));
        if i < window_end {
            spans.push(Span::styled(" ".to_owned(), Style::default().bg(colors.bg)));
        }
    }

    // Right marker – omitted when nothing is hidden on that side.
    if right_count > 0 {
        let s = if right_count >= 99 {
            "..+99".to_owned()
        } else {
            format!("...{:2}", right_count)
        };
        spans.push(Span::styled(s, dim_s));
    }

    // Centre the assembled content in the row.
    let content_width: usize = spans.iter().map(|s| s.content.width()).sum();
    let pad = render_width.saturating_sub(content_width) / 2;
    let mut all: Vec<Span<'static>> =
        vec![Span::styled(" ".repeat(pad), Style::default().bg(colors.bg))];
    all.extend(spans);

    f.render_widget(
        Paragraph::new(Line::from(all)).style(Style::default().bg(colors.bg)),
        area,
    );
}

// ── Hint bar ──────────────────────────────────────────────────────────────────

// Display widths for Normal-mode hint pairs (key + description):
//   [r] Refresh   = 3 + 10 = 13
//   [x] Delete    = 3 +  9 = 12
//   [c] Clear all = 3 + 12 = 15
//   [?] Help      = 3 +  5 =  8  (never hidden)
const HINT_W_HELP: usize = 8;
const HINT_W_R: usize = 13;
const HINT_W_X: usize = 12;
const HINT_W_C: usize = 15;

fn render_hints(f: &mut Frame, app: &App, area: Rect, colors: &AppColors) {
    let key_s = Style::default().fg(colors.accent).bg(colors.bg);
    let desc_s = Style::default().fg(colors.fg).bg(colors.bg);

    let width = area.width as usize;

    let pairs: Vec<(&str, &str)> = match app.mode {
        Mode::Normal => {
            // Progressively hide [c], [x], [r] as the terminal narrows. [?] is never hidden.
            let show_c = width >= HINT_W_HELP + HINT_W_R + HINT_W_X + HINT_W_C;
            let show_x = width >= HINT_W_HELP + HINT_W_R + HINT_W_X;
            let show_r = width >= HINT_W_HELP + HINT_W_R;
            let mut v: Vec<(&str, &str)> = Vec::new();
            if show_r { v.push(("[r]", " Refresh  ")); }
            if show_x { v.push(("[x]", " Delete  ")); }
            if show_c { v.push(("[c]", " Clear all  ")); }
            v.push(("[?]", " Help"));
            v
        }
        Mode::Filter => vec![("[Enter]", " Confirm  "), ("[Esc]", " Cancel")],
        Mode::Help => vec![("[Esc]", " Close")],
        _ => vec![],
    };

    let spans: Vec<Span<'static>> = pairs
        .iter()
        .flat_map(|(k, d)| {
            [
                Span::styled(k.to_string(), key_s),
                Span::styled(d.to_string(), desc_s),
            ]
        })
        .collect();

    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(colors.bg)),
        area,
    );
}

// ── Filter bar ────────────────────────────────────────────────────────────────

fn render_filter_bar(f: &mut Frame, app: &App, area: Rect, colors: &AppColors) {
    let line = Line::from(vec![
        Span::styled("Filter: ".to_owned(), Style::default().fg(colors.accent).bg(colors.bg)),
        Span::styled(
            app.filter_input.clone(),
            Style::default().fg(colors.fg).bg(colors.highlight),
        ),
        Span::styled(
            "█".to_owned(),
            Style::default().fg(colors.accent).bg(colors.bg),
        ),
    ]);
    f.render_widget(
        Paragraph::new(line).style(Style::default().bg(colors.bg)),
        area,
    );
}

// ── Confirm dialog ────────────────────────────────────────────────────────────

fn render_confirm_dialog(f: &mut Frame, area: Rect, colors: &AppColors, message: &str) {
    let buttons_combined = "[y/Enter] Yes  [n/Esc] No";
    let button_yes = "[y/Enter] Yes";
    let button_no = "[n/Esc] No";

    // Maximum inner width available (terminal width minus two border columns).
    let max_inner_w = (area.width as usize).saturating_sub(2);

    // Determine the best layout, trying progressively more compact arrangements.
    //
    //   1. Single line:  "message — [y/Enter] Yes  [n/Esc] No"
    //   2. Two lines:    message / combined buttons  (word-wrap message if needed)
    //   3. Three lines:  word-wrapped message / "[y/Enter] Yes" / "[n/Esc] No"
    let content_lines: Vec<String> = {
        let single = format!("{} \u{2014} {}", message, buttons_combined);

        if single.width() <= max_inner_w {
            // Layout 1 – everything on one line.
            vec![single]
        } else {
            let wrapped_msg = word_wrap_to_cols(message, max_inner_w);
            let mut with_combined = wrapped_msg.clone();
            with_combined.push(buttons_combined.to_string());

            if with_combined.iter().all(|l| l.width() <= max_inner_w) {
                // Layout 2 – word-wrapped message then combined buttons.
                with_combined
            } else {
                // Layout 3 – word-wrapped message then each button on its own line.
                let mut v = word_wrap_to_cols(message, max_inner_w);
                v.push("".to_string());
                v.push(button_yes.to_string());
                v.push(button_no.to_string());
                v
            }
        }
    };

    // Popup dimensions: just wide enough for the longest line, tall enough for all lines.
    let max_line_w = content_lines.iter().map(|l| l.width()).max().unwrap_or(0);
    let popup_w = (max_line_w + 2).min(area.width as usize) as u16;
    let popup_h = (content_lines.len() + 2).min(area.height as usize) as u16;

    if popup_w < 2 || popup_h < 2 {
        return;
    }

    let popup_x = area.x + area.width.saturating_sub(popup_w) / 2;
    let popup_y = area.y + area.height.saturating_sub(popup_h) / 2;
    let popup = Rect::new(popup_x, popup_y, popup_w, popup_h);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(colors.accent))
        .style(Style::default().bg(colors.bg));
    let inner = block.inner(popup);

    f.render_widget(Clear, popup);
    f.render_widget(block, popup);

    let inner_w = inner.width as usize;
    let fg_s = Style::default().fg(colors.fg).bg(colors.bg);

    // Center each line horizontally within the inner area.
    let text: Vec<Line<'static>> = content_lines
        .into_iter()
        .take(inner.height as usize)
        .map(|l| {
            let line_w = l.width();
            let pad = inner_w.saturating_sub(line_w) / 2;
            let padded = format!("{}{}", " ".repeat(pad), l);
            Line::from(Span::styled(padded, fg_s))
        })
        .collect();

    f.render_widget(
        Paragraph::new(Text::from(text)).style(Style::default().bg(colors.bg)),
        inner,
    );
}

// ── Help popup ────────────────────────────────────────────────────────────────

fn render_help_popup(f: &mut Frame, app: &mut App, area: Rect, colors: &AppColors) {
    const TITLE: &str = " Keybindings ";
    // Minimum popup width so the title is not clipped by the border corners.
    const TITLE_MIN_W: usize = TITLE.len() + 2;
    // Sentinel stored as the "key" field of a display row to mark continuation lines
    // (second and later wrapped lines of a single entry's description).
    const CONTINUATION: &str = "\x00";

    // Structured entries: (key, description).  Both empty ⟹ blank separator line.
    let entries: &[(&str, &str)] = &[
        ("r", "Refresh"),
        ("c", "Clear all"),
        ("x", "Delete current"),
        ("q", "Quit"),
        ("g", "Go to start"),
        ("G", "Go to end"),
        ("/", "Filter"),
        ("s", "Toggle select"),
        ("S", "Delete selected"),
        ("?", "Show keybinds"),
        ("F1", "Toggle hint bar"),
        ("↑ / k", "Move up"),
        ("↓ / j", "Move down"),
        ("← / h", "Previous page"),
        ("→ / l", "Next page"),
        ("PgUp", "Previous page"),
        ("PgDn", "Next page"),
        ("", ""),
        ("Esc", "Close help"),
    ];

    // Key column = widest key display width.
    let key_col: usize = entries.iter().map(|(k, _)| k.width()).max().unwrap_or(0);

    // Row layout: 1 leading space + key (padded to key_col) + 1 sep space + desc.
    // overhead = columns consumed before the description text begins.
    let overhead: usize = 1 + key_col + 1;

    // Maximum inner width we can actually use (terminal width minus two border cols).
    let max_inner_w = (area.width as usize).saturating_sub(2);
    let desc_avail = max_inner_w.saturating_sub(overhead);

    // ── Build display rows (key, desc_line) ──────────────────────────────────
    // key == "" && desc == ""  → blank separator
    // key == ""  && desc != "" → continuation line (indent only, no key rendered)
    // key != ""                → first line of an entry
    let mut display_rows: Vec<(&str, String)> = Vec::new();

    for &(key, desc) in entries {
        // Blank separator.
        if key.is_empty() && desc.is_empty() {
            display_rows.push(("", String::new()));
            continue;
        }

        if desc.is_empty() || desc_avail == 0 {
            // No room (or no content) for a description – show key only.
            display_rows.push((key, String::new()));
            continue;
        }

        if desc.width() <= desc_avail {
            // Description fits on one line – no wrapping needed.
            display_rows.push((key, desc.to_string()));
        } else {
            // Step 2: word-wrap the description.
            let wrapped = word_wrap_to_cols(desc, desc_avail);
            for (i, mut line) in wrapped.into_iter().enumerate() {
                // Step 3: truncate any wrapped line that is still too long
                // (happens when a single word exceeds desc_avail).
                if line.width() > desc_avail {
                    line = if desc_avail >= 4 {
                        let t = truncate_to_cols(&line, desc_avail.saturating_sub(3));
                        format!("{t}...")
                    } else {
                        truncate_to_cols(&line, desc_avail)
                    };
                }
                let sentinel = if i == 0 { key } else { CONTINUATION };
                display_rows.push((sentinel, line));
            }
        }
    }

    // ── Compute popup width ───────────────────────────────────────────────────
    let max_row_w: usize = display_rows
        .iter()
        .map(|(k, d)| {
            if k.is_empty() && d.is_empty() {
                0 // blank separator
            } else {
                overhead + d.width()
            }
        })
        .max()
        .unwrap_or(0);

    let popup_w = (max_row_w + 2).max(TITLE_MIN_W).min(area.width as usize) as u16;
    let inner_w = popup_w.saturating_sub(2) as usize;

    // ── Pagination ────────────────────────────────────────────────────────────
    let total_rows = display_rows.len();
    // Maximum inner height this popup can occupy.
    let max_inner_h = (area.height as usize).saturating_sub(2).max(1);

    let (rows_per_page, total_pages, show_footer) = if total_rows <= max_inner_h {
        (max_inner_h, 1usize, false)
    } else {
        // Reserve one inner row for the "← X/N pages →" footer.
        let rpp = max_inner_h.saturating_sub(1).max(1);
        let tp = total_rows.div_ceil(rpp);
        (rpp, tp, true)
    };

    app.help_total_pages = total_pages;
    if app.help_page >= total_pages {
        app.help_page = total_pages.saturating_sub(1);
    }

    let page_start = app.help_page * rows_per_page;
    let page_end = (page_start + rows_per_page).min(total_rows);
    let page_row_count = page_end - page_start;

    let inner_h = page_row_count + usize::from(show_footer);
    let popup_h = (inner_h + 2).min(area.height as usize) as u16;

    if popup_w < 2 || popup_h < 2 {
        return;
    }

    let popup_x = area.x + area.width.saturating_sub(popup_w) / 2;
    let popup_y = area.y + area.height.saturating_sub(popup_h) / 2;
    let popup = Rect::new(popup_x, popup_y, popup_w, popup_h);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(TITLE)
        .border_style(Style::default().fg(colors.accent))
        .style(Style::default().bg(colors.bg));
    let inner_rect = block.inner(popup);

    f.render_widget(Clear, popup);
    f.render_widget(block, popup);

    let fg_s = Style::default().fg(colors.fg).bg(colors.bg);
    let key_s = Style::default().fg(colors.accent).bg(colors.bg);
    let dim_s = Style::default()
        .fg(colors.fg)
        .bg(colors.bg)
        .add_modifier(Modifier::DIM);
    let accent_s = Style::default().fg(colors.accent).bg(colors.bg);

    let mut lines: Vec<Line<'static>> = Vec::new();

    for (sentinel, desc) in display_rows.iter().skip(page_start).take(page_row_count) {
        let line = if sentinel.is_empty() && desc.is_empty() {
            // Blank separator row.
            Line::from(Span::styled(String::new(), fg_s))
        } else if *sentinel == CONTINUATION {
            // Continuation line – indent to align with the description column.
            Line::from(vec![
                Span::styled(" ".repeat(overhead), fg_s),
                Span::styled(desc.clone(), fg_s),
            ])
        } else {
            // First line of an entry: key (accent) then description.
            let key_pad = key_col.saturating_sub(sentinel.width());
            Line::from(vec![
                Span::styled(" ".to_string(), fg_s),
                Span::styled(sentinel.to_string(), key_s),
                Span::styled(" ".repeat(key_pad + 1), fg_s),
                Span::styled(desc.clone(), fg_s),
            ])
        };
        lines.push(line);
    }

    // ── Pagination footer (only when there is more than one page) ─────────────
    if show_footer {
        let page_str = format!("{}/{} pages", app.help_page + 1, total_pages);
        let page_str_w = page_str.width();
        let has_prev = app.help_page > 0;
        let has_next = app.help_page + 1 < total_pages;

        // Layout: [←][pad_l][page_str][pad_r][→]
        // Total = 1 + pad_l + page_str_w + pad_r + 1 = inner_w
        let total_space = inner_w.saturating_sub(2 + page_str_w);
        let pad_l = total_space / 2;
        let pad_r = total_space - pad_l;

        let left_arrow = if has_prev { "←" } else { " " };
        let right_arrow = if has_next { "→" } else { " " };

        lines.push(Line::from(vec![
            Span::styled(left_arrow.to_string(), accent_s),
            Span::styled(" ".repeat(pad_l), dim_s),
            Span::styled(page_str, dim_s),
            Span::styled(" ".repeat(pad_r), dim_s),
            Span::styled(right_arrow.to_string(), accent_s),
        ]));
    }

    f.render_widget(
        Paragraph::new(Text::from(lines)).style(Style::default().bg(colors.bg)),
        inner_rect,
    );
}

// ── Fuzzy-match highlighting ──────────────────────────────────────────────────

/// Split `text` into styled spans, highlighting characters at positions in `indices`.
fn styled_with_matches(
    text: &str,
    indices: &[usize],
    normal: Style,
    matched: Style,
) -> Vec<Span<'static>> {
    if indices.is_empty() {
        return vec![Span::styled(text.to_owned(), normal)];
    }

    let idx_set: std::collections::HashSet<usize> = indices.iter().copied().collect();
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut buf = String::new();
    let mut in_match = false;

    for (i, ch) in text.chars().enumerate() {
        let is_match = idx_set.contains(&i);
        if is_match != in_match {
            if !buf.is_empty() {
                let s = if in_match { matched } else { normal };
                spans.push(Span::styled(buf.clone(), s));
                buf.clear();
            }
            in_match = is_match;
        }
        buf.push(ch);
    }
    if !buf.is_empty() {
        spans.push(Span::styled(buf, if in_match { matched } else { normal }));
    }
    spans
}

// ── Terminal-too-small screen ─────────────────────────────────────────────────

/// Render a plain "terminal too small" message instead of the normal TUI.
/// The message is character-wrapped to fit whatever width is available.
pub fn draw_too_small(f: &mut Frame, min_w: u16, min_h: u16) {
    let area = f.area();
    let msg = format!(
        "Terminal too small to function, please resize the terminal window to at least {min_w} x {min_h}."
    );

    let wrap_w = area.width as usize;
    let wrapped: Vec<String> = if wrap_w == 0 {
        vec![msg]
    } else {
        wrap_to_cols(&msg, wrap_w)
            .into_iter()
            .map(|(s, _)| s)
            .collect()
    };

    let lines: Vec<Line<'static>> = wrapped
        .into_iter()
        .map(|l| Line::from(Span::raw(l)))
        .collect();

    f.render_widget(Clear, area);
    f.render_widget(Paragraph::new(Text::from(lines)), area);
}
