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

    // Modal overlays
    match &app.mode {
        Mode::ConfirmClearAll => {
            render_confirm_dialog(f, area, &colors, "Clear ALL notifications?");
        }
        Mode::ConfirmClearSelected => {
            render_confirm_dialog(f, area, &colors, "Clear SELECTED notifications?");
        }
        Mode::Help => render_help_popup(f, area, &colors),
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

// ── Notification list ─────────────────────────────────────────────────────────

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
    let full_msg = format!("{} — [y/Enter] Yes  [n/Esc] No", message);
    let inner_width = full_msg.len() as u16 + 2;
    let popup_w = (inner_width + 2).min(area.width.saturating_sub(2));
    let popup_h = 3u16;
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
    f.render_widget(
        Paragraph::new(full_msg).style(Style::default().fg(colors.fg).bg(colors.bg)),
        inner,
    );
}

// ── Help popup ────────────────────────────────────────────────────────────────

fn render_help_popup(f: &mut Frame, area: Rect, colors: &AppColors) {
    let rows: &[&str] = &[
        " r        Refresh",
        " c        Clear all",
        " x        Delete current",
        " q        Quit",
        " g        Go to start",
        " G        Go to end",
        " /        Filter",
        " s        Toggle select",
        " S        Delete selected",
        " ?        Show keybinds",
        " F1       Toggle hint bar",
        " ↑ / k    Move up",
        " ↓ / j    Move down",
        " ← / h    Previous page",
        " → / l    Next page",
        " PgUp     Previous page",
        " PgDn     Next page",
        "",
        " Esc      Close help",
    ];

    let popup_w: u16 = 32;
    let popup_h: u16 = (rows.len() as u16 + 2).min(area.height.saturating_sub(2));
    let popup_x = area.x + area.width.saturating_sub(popup_w) / 2;
    let popup_y = area.y + area.height.saturating_sub(popup_h) / 2;
    let popup = Rect::new(popup_x, popup_y, popup_w, popup_h);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Keybindings ")
        .border_style(Style::default().fg(colors.accent))
        .style(Style::default().bg(colors.bg));
    let inner = block.inner(popup);

    let text: Vec<Line<'static>> = rows
        .iter()
        .take(inner.height as usize)
        .map(|row| {
            Line::from(Span::styled(
                row.to_string(),
                Style::default().fg(colors.fg).bg(colors.bg),
            ))
        })
        .collect();

    f.render_widget(Clear, popup);
    f.render_widget(block, popup);
    f.render_widget(
        Paragraph::new(Text::from(text)).style(Style::default().bg(colors.bg)),
        inner,
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
