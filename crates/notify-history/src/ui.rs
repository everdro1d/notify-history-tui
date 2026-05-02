use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

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

    // Build layout constraints (header | gap | content | [filter] | dots | hints)
    let mut constraints: Vec<Constraint> = vec![
        Constraint::Length(1), // header
        Constraint::Length(1), // blank gap below header
        Constraint::Fill(1),   // notification list
    ];
    if in_filter {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Length(1)); // dots row (blank when 1 page)
    constraints.push(Constraint::Length(1)); // hints bar

    let chunks = Layout::vertical(constraints).split(area);
    let header_area = chunks[0];
    // chunks[1] is the blank gap — nothing rendered there
    let content_area = chunks[2];

    // Must happen before rendering so pagination is correct
    app.update_items_per_page(content_area.height);

    let mut chunk_idx: usize = 3;

    if in_filter {
        render_filter_bar(f, app, chunks[chunk_idx], &colors);
        chunk_idx += 1;
    }

    let dots_area = chunks[chunk_idx];
    chunk_idx += 1;
    let hints_area = chunks[chunk_idx];

    render_header(f, app, header_area, &colors);
    render_notifications(f, app, content_area, &colors);
    render_dots(f, app, dots_area, &colors);
    render_hints(f, app, hints_area, &colors);

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
    let title = if area.width >= 20 {
        "Notification History"
    } else if area.width >= 13 {
        "Notifications"
    } else {
        "Notifs."
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

// ── Notification list ─────────────────────────────────────────────────────────

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
                "▶ "
            } else {
                "  "
            };

            let mut spans: Vec<Span<'static>> = vec![Span::styled(marker.to_owned(), accent)];
            spans.extend(styled_with_matches(
                &notif.summary,
                &match_idx.summary,
                normal,
                match_style,
            ));
            if show_app && !notif.app_name.is_empty() {
                spans.push(Span::styled(" — ".to_owned(), normal));
                spans.extend(styled_with_matches(
                    &notif.app_name,
                    &match_idx.app_name,
                    accent,
                    match_style,
                ));
            }
            lines.push(Line::from(spans).style(normal));

            // ── Lines 2 … (1+body_lines): body ─────────────────────────────
            if body_lines_count > 0 {
                let body_text_lines: Vec<&str> = notif.body.lines().collect();
                let empty: Vec<usize> = Vec::new();
                for line_i in 0..body_lines_count {
                    let text = body_text_lines.get(line_i).copied().unwrap_or("");
                    let midx = match_idx.body_per_line.get(line_i).unwrap_or(&empty);
                    let mut row: Vec<Span<'static>> =
                        vec![Span::styled("   ".to_owned(), normal)];
                    row.extend(styled_with_matches(text, midx, normal, match_style));
                    lines.push(Line::from(row).style(normal));
                }
            }

            // ── Last body line: datetime ────────────────────────────────────
            lines.push(
                Line::from(vec![
                    Span::styled("   ".to_owned(), dim),
                    Span::styled(notif.datetime_str(), dim),
                ])
                .style(dim),
            );

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

fn render_dots(f: &mut Frame, app: &App, area: Rect, colors: &AppColors) {
    let n = app.num_pages();
    if n <= 1 {
        f.render_widget(
            Paragraph::new("").style(Style::default().bg(colors.bg)),
            area,
        );
        return;
    }

    // Build dot spans
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

    // Center: each dot = 1 col, each space = 1 col → total = n * 2 - 1
    let total_width = n * 2 - 1;
    let pad = (area.width as usize).saturating_sub(total_width) / 2;
    let mut all: Vec<Span<'static>> = vec![Span::styled(
        " ".repeat(pad),
        Style::default().bg(colors.bg),
    )];
    all.extend(dot_spans);

    f.render_widget(
        Paragraph::new(Line::from(all)).style(Style::default().bg(colors.bg)),
        area,
    );
}

// ── Hint bar ──────────────────────────────────────────────────────────────────

fn render_hints(f: &mut Frame, app: &App, area: Rect, colors: &AppColors) {
    let key_s = Style::default().fg(colors.accent).bg(colors.bg);
    let desc_s = Style::default().fg(colors.fg).bg(colors.bg);

    let pairs: &[(&str, &str)] = match app.mode {
        Mode::Normal => &[
            ("[r]", " Refresh  "),
            ("[x]", " Delete  "),
            ("[c]", " Clear all  "),
            ("[?]", " Help"),
        ],
        Mode::Filter => &[("[Enter]", " Confirm  "), ("[Esc]", " Cancel")],
        Mode::Help => &[("[Esc]", " Close")],
        _ => &[],
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
