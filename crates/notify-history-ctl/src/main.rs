use clap::{CommandFactory, Parser};
use clap_complete::{generate, Shell};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const PID_FILE: &str = "/tmp/notify-history-ctl.pid";
const DELIMITER: &str = "[*]";
const DEFAULT_HISTORY_FILE: &str = "/tmp/notification-history";

#[derive(Parser)]
#[command(
    name = "notify-history-ctl",
    about = "Notification history daemon — monitors D-Bus and records notifications"
)]
struct Args {
    /// Stop the running daemon
    #[arg(long)]
    stop: bool,

    /// Clear all notification history
    #[arg(long = "clear-history")]
    clear_history: bool,

    /// Generate shell completions and print to stdout
    #[arg(long = "generate", value_name = "SHELL", hide = true)]
    generate: Option<Shell>,
}

// ── Notification ──────────────────────────────────────────────────────────────

struct Notification {
    timestamp: i64,
    app_name: String,
    summary: String,
    body: String,
}

impl Notification {
    fn to_line(&self) -> String {
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
}

// ── dbus-monitor parser ───────────────────────────────────────────────────────

#[derive(Default)]
struct NotifyCollecting {
    timestamp: i64,
    /// index of the next Notify parameter we expect (0-4)
    field_idx: usize,
    app_name: String,
    summary: String,
    body: String,
    /// buffer for a string value that spans multiple output lines
    string_buf: Option<String>,
}

enum ParseState {
    Idle,
    Collecting(NotifyCollecting),
}

fn process_line(line: &str, state: &mut ParseState) -> Option<Notification> {
    // ── Multi-line string continuation ───────────────────────────────────────
    // Must be checked BEFORE the indentation guard because dbus-monitor prints
    // continuation lines without any leading spaces.
    if let ParseState::Collecting(ref mut s) = state {
        if s.string_buf.is_some() {
            if ends_with_unescaped_quote(line) {
                let mut buf = s.string_buf.take().unwrap();
                buf.push('\n');
                buf.push_str(&line[..line.len() - 1]);
                let value = escape_value(&buf);
                assign_field(s.field_idx, &value, &mut s.app_name, &mut s.summary, &mut s.body);
                s.field_idx += 1;
                if s.field_idx >= 5 {
                    return Some(finish(s));
                }
            } else {
                let buf = s.string_buf.as_mut().unwrap();
                buf.push('\n');
                buf.push_str(line);
            }
            return None;
        }
    }

    // Header lines start without leading spaces
    if !line.starts_with("   ") {
        if line.contains("member=Notify") {
            *state = ParseState::Collecting(NotifyCollecting {
                timestamp: extract_timestamp(line),
                ..Default::default()
            });
        } else {
            *state = ParseState::Idle;
        }
        return None;
    }

    let s = match state {
        ParseState::Collecting(ref mut s) => s,
        ParseState::Idle => return None,
    };

    if s.field_idx >= 5 {
        return None;
    }

    let trimmed = line.trim();

    if let Some(after_quote) = trimmed.strip_prefix("string \"") {
        if ends_with_unescaped_quote(after_quote) {
            // single-line string — strip trailing "
            let value = escape_value(&after_quote[..after_quote.len() - 1]);
            assign_field(s.field_idx, &value, &mut s.app_name, &mut s.summary, &mut s.body);
            s.field_idx += 1;
            if s.field_idx >= 5 {
                return Some(finish(s));
            }
        } else {
            // start of multi-line string
            s.string_buf = Some(after_quote.to_string());
        }
    } else if is_primitive_field(trimmed) {
        // uint32/int32/boolean/byte — consume without storing
        s.field_idx += 1;
    }
    // array / dict / variant lines are ignored (they appear at field_idx >= 5)

    None
}

fn assign_field(field_idx: usize, value: &str, app_name: &mut String, summary: &mut String, body: &mut String) {
    match field_idx {
        0 => *app_name = value.to_string(),
        // 1 = replaces_id (uint32) — incremented via is_primitive_field
        // 2 = app_icon (string)    — incremented but not stored
        3 => *summary = value.to_string(),
        4 => *body = value.to_string(),
        _ => {}
    }
}

fn finish(s: &NotifyCollecting) -> Notification {
    Notification {
        timestamp: s.timestamp,
        app_name: s.app_name.clone(),
        summary: s.summary.clone(),
        body: s.body.clone(),
    }
}

fn ends_with_unescaped_quote(s: &str) -> bool {
    if !s.ends_with('"') {
        return false;
    }
    let trailing_backslashes = s[..s.len() - 1]
        .chars()
        .rev()
        .take_while(|&c| c == '\\')
        .count();
    trailing_backslashes % 2 == 0
}

fn is_primitive_field(trimmed: &str) -> bool {
    trimmed.starts_with("uint32 ")
        || trimmed.starts_with("int32 ")
        || trimmed.starts_with("boolean ")
        || trimmed.starts_with("byte ")
        || trimmed.starts_with("int64 ")
        || trimmed.starts_with("uint64 ")
}

fn extract_timestamp(line: &str) -> i64 {
    for token in line.split_whitespace() {
        if let Some(ts_str) = token.strip_prefix("time=") {
            let mut parts = ts_str.splitn(2, '.');
            let secs_str = parts.next().unwrap_or("");
            let frac_str = parts.next().unwrap_or("0");
            if let Ok(secs) = secs_str.parse::<i64>() {
                return secs * 1000 + frac_to_ms(frac_str);
            }
        }
    }
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Convert the fractional-seconds string (e.g. "123456" from dbus-monitor's
/// microsecond-precision `time=` field) to whole milliseconds.
fn frac_to_ms(frac: &str) -> i64 {
    // Pad or truncate to exactly 3 significant digits then parse.
    let truncated = &frac[..frac.len().min(3)];
    let padded = format!("{:0<3}", truncated);
    padded.parse::<i64>().unwrap_or(0)
}

/// Escape special characters so each notification fits on a single file line.
fn escape_value(s: &str) -> String {
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

fn prepend_to_file(path: &PathBuf, content: &str, max_history: usize) -> io::Result<()> {
    let existing = if path.exists() {
        fs::read_to_string(path)?
    } else {
        String::new()
    };
    let mut file = File::create(path)?;
    writeln!(file, "{}", content)?;
    if max_history > 0 {
        // Write only up to max_history - 1 existing lines (the new line already counts as 1)
        for line in existing.lines().take(max_history) {
            writeln!(file, "{}", line)?;
        }
    } else {
        file.write_all(existing.as_bytes())?;
    }
    Ok(())
}

// ── Config helper ─────────────────────────────────────────────────────────────

fn get_config_values() -> (PathBuf, usize) {
    if let Some(home) = dirs::home_dir() {
        let config = home.join(".config").join("notify-history").join("config.toml");
        if let Ok(content) = fs::read_to_string(&config) {
            let history_file = if is_persistence_enabled(&content) {
                let state_dir = dirs::state_dir()
                    .unwrap_or_else(|| home.join(".local").join("state"));
                state_dir.join("notify-history").join("notification-history")
            } else {
                PathBuf::from(DEFAULT_HISTORY_FILE)
            };
            let max_history = get_max_history(&content);
            return (history_file, max_history);
        }
    }
    (PathBuf::from(DEFAULT_HISTORY_FILE), 0)
}

fn get_history_file() -> PathBuf {
    get_config_values().0
}

fn is_persistence_enabled(content: &str) -> bool {
    let mut in_section = false;
    for line in content.lines() {
        let t = line.trim();
        if t == "[persistence]" {
            in_section = true;
        } else if t.starts_with('[') {
            in_section = false;
        } else if in_section {
            let norm = t.replace(' ', "");
            if norm == "enabled=true" {
                return true;
            }
        }
    }
    false
}

fn get_max_history(content: &str) -> usize {
    let mut in_section = false;
    for line in content.lines() {
        let t = line.trim();
        if t == "[persistence]" {
            in_section = true;
        } else if t.starts_with('[') {
            in_section = false;
        } else if in_section {
            let norm = t.replace(' ', "");
            if let Some(val) = norm.strip_prefix("max_history=") {
                return val.parse::<usize>().unwrap_or(0);
            }
        }
    }
    0
}

// ── Actions ───────────────────────────────────────────────────────────────────

fn stop_daemon() {
    match fs::read_to_string(PID_FILE) {
        Ok(content) => {
            let pid = content.trim();
            if pid.is_empty() {
                eprintln!("PID file is empty");
                std::process::exit(1);
            }
            match Command::new("kill").arg(pid).status() {
                Ok(s) if s.success() => println!("Daemon stopped (PID {})", pid),
                Ok(_) => {
                    eprintln!("Failed to stop daemon — process may no longer exist");
                    let _ = fs::remove_file(PID_FILE);
                }
                Err(e) => eprintln!("Error sending signal: {}", e),
            }
        }
        Err(_) => {
            eprintln!("No running daemon found (PID file not present)");
            std::process::exit(1);
        }
    }
}

fn clear_history() {
    let path = get_history_file();
    match OpenOptions::new().write(true).truncate(true).create(true).open(&path) {
        Ok(_) => println!("History cleared: {}", path.display()),
        Err(e) => {
            eprintln!("Error clearing history: {}", e);
            std::process::exit(1);
        }
    }
}

fn run_daemon() {
    let pid = std::process::id();
    if let Err(e) = fs::write(PID_FILE, pid.to_string()) {
        eprintln!("Warning: could not write PID file: {}", e);
    }

    let (history_file, max_history) = get_config_values();
    if let Some(parent) = history_file.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let running = Arc::new(AtomicBool::new(true));
    let running_signal = running.clone();
    let pid_file_signal = PID_FILE.to_string();

    ctrlc::set_handler(move || {
        running_signal.store(false, Ordering::SeqCst);
        let _ = fs::remove_file(&pid_file_signal);
        std::process::exit(0);
    })
    .expect("Error setting signal handler");

    let mut child = match Command::new("dbus-monitor")
        .arg("interface='org.freedesktop.Notifications'")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to spawn dbus-monitor: {}", e);
            eprintln!("Ensure dbus-monitor is installed (part of the dbus package).");
            let _ = fs::remove_file(PID_FILE);
            std::process::exit(1);
        }
    };

    let stdout = child.stdout.take().expect("Failed to get dbus-monitor stdout");
    let reader = BufReader::new(stdout);
    let mut state = ParseState::Idle;

    for line_result in reader.lines() {
        if !running.load(Ordering::SeqCst) {
            break;
        }
        match line_result {
            Ok(line) => {
                if let Some(notif) = process_line(&line, &mut state) {
                    if let Err(e) = prepend_to_file(&history_file, &notif.to_line(), max_history) {
                        eprintln!("Failed to write notification: {}", e);
                    }
                }
            }
            Err(_) => break,
        }
    }

    let _ = child.wait();
    let _ = fs::remove_file(PID_FILE);
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    let args = Args::parse();

    if let Some(shell) = args.generate {
        let mut cmd = Args::command();
        generate(shell, &mut cmd, "notify-history-ctl", &mut io::stdout());
        return;
    }

    if args.stop {
        stop_daemon();
        return;
    }

    if args.clear_history {
        clear_history();
        return;
    }

    run_daemon();
}
