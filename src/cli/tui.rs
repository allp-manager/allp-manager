use crate::{
    domain::{ExecutionPlan, OperationStatus, RuntimePrivilegeContext},
    execution::{ExecutionObserver, ProcessEvent, ProcessOutputStream},
};
use std::{
    env,
    io::{self, Write},
    time::Duration,
};

const DEFAULT_WIDTH: usize = 80;
const MIN_WIDTH: usize = 20;
const MAX_WIDTH: usize = 240;
const MIN_BAR_WIDTH: usize = 8;
const MAX_BAR_WIDTH: usize = 28;
const MAX_PENDING_OUTPUT: usize = 64 * 1024;

/// A single-line, apt-style progress display for maintenance operations.
///
/// The normal terminal buffer remains in use. Native output scrolls normally
/// and this observer owns only the current, unterminated line. Before writing
/// output or yielding the terminal to a prompt, that line is removed.
pub struct MaintenanceTui {
    total: usize,
    completed: usize,
    active: Option<ActiveOperation>,
    color: bool,
    width: usize,
    stdout_pending: String,
    stderr_pending: String,
    footer_visible: bool,
    io_failed: bool,
}

#[derive(Debug, Clone)]
struct ActiveOperation {
    backend_name: String,
    action: String,
    elapsed: Duration,
    percent: Option<u8>,
}

impl MaintenanceTui {
    pub fn new(_operation: &str, total: usize, color: bool) -> Self {
        let mut tui = Self {
            total: total.max(1),
            completed: 0,
            active: None,
            color,
            width: terminal_width(),
            stdout_pending: String::new(),
            stderr_pending: String::new(),
            footer_visible: false,
            io_failed: false,
        };
        tui.draw_footer();
        tui
    }

    pub fn start_operation(
        &mut self,
        _index: usize,
        total: usize,
        plan: &ExecutionPlan,
        _privilege_context: &RuntimePrivilegeContext,
    ) {
        self.total = total.max(1);
        self.flush_pending();
        self.active = Some(ActiveOperation {
            backend_name: plan.backend_name.clone(),
            action: plan.action.clone(),
            elapsed: Duration::ZERO,
            percent: None,
        });
        self.draw_footer();
    }

    pub fn finish_operation(
        &mut self,
        index: usize,
        total: usize,
        _backend_name: &str,
        _status: &OperationStatus,
        _message: Option<&str>,
        _elapsed: Duration,
    ) {
        self.total = total.max(1);
        self.flush_pending();
        self.completed = self.completed.max(index).min(self.total);
        self.active = None;
        self.draw_footer();
    }

    pub fn record_outcome(
        &mut self,
        index: usize,
        total: usize,
        _backend_name: &str,
        _status: &OperationStatus,
        _message: Option<&str>,
    ) {
        self.total = total.max(1);
        self.flush_pending();
        self.completed = self.completed.max(index).min(self.total);
        self.active = None;
        self.draw_footer();
    }

    pub fn set_total(&mut self, total: usize) {
        self.total = total.max(1);
        self.draw_footer();
    }

    /// Removes the progress line and moves prompts onto an ordinary fresh line.
    pub fn prepare_for_prompt(&mut self) {
        self.flush_pending();
        self.clear_footer();
        self.write_raw("\n");
    }

    /// Redraws progress only after the terminal prompt has fully completed.
    pub fn resume_after_prompt(&mut self) {
        self.draw_footer();
    }

    pub fn queue_extended(&mut self, total: usize, _backend_name: &str, _added: usize) {
        self.set_total(total);
    }

    /// Permanently removes the live line. The caller can then print the normal
    /// summary without the progress renderer changing its contents.
    pub fn finish(&mut self) {
        self.flush_pending();
        self.completed = self.total;
        self.active = None;
        self.draw_footer();
        self.clear_footer();
    }

    fn draw_footer(&mut self) {
        if self.io_failed {
            return;
        }

        self.width = terminal_width();
        let percentage = self.overall_percentage();
        let bar_width = self
            .width
            .saturating_sub(42)
            .clamp(MIN_BAR_WIDTH, MAX_BAR_WIDTH);
        let filled = usize::from(percentage) * bar_width / 100;
        let bar = format!("[{}{}]", "#".repeat(filled), ".".repeat(bar_width - filled));
        let detail = self.active.as_ref().map_or_else(
            || format!("{}/{} complete", self.completed.min(self.total), self.total),
            |active| {
                format!(
                    "{} · {} · {}/{} · {}",
                    active.backend_name,
                    active.action,
                    self.completed.min(self.total),
                    self.total,
                    format_duration(active.elapsed)
                )
            },
        );
        let prefix = format!("Progress: [{percentage:>3}%] {bar} ");
        let max_visible_width = self.width.saturating_sub(1).max(1);
        let detail_width = max_visible_width.saturating_sub(display_width(&prefix));
        let text = truncate(
            &format!("{prefix}{}", truncate(&detail, detail_width)),
            max_visible_width,
        );

        self.write_raw("\r\x1b[2K");
        if self.color {
            self.write_raw(&format!("\x1b[36m{text}\x1b[0m"));
        } else {
            self.write_raw(&text);
        }
        self.footer_visible = !self.io_failed;
    }

    fn overall_percentage(&self) -> u8 {
        let total = self.total.max(1);
        let completed = self.completed.min(total);
        let active = self
            .active
            .as_ref()
            .and_then(|active| active.percent)
            .map(usize::from)
            .unwrap_or(0);
        (((completed * 100 + active) / total).min(100)) as u8
    }

    fn clear_footer(&mut self) {
        if self.footer_visible {
            self.write_raw("\r\x1b[2K");
            self.footer_visible = false;
        }
    }

    fn write_log_line(&mut self, line: &str) {
        self.clear_footer();
        self.write_raw(line);
        self.write_raw("\n");
        self.draw_footer();
    }

    fn flush_pending(&mut self) {
        let stdout = std::mem::take(&mut self.stdout_pending);
        let stderr = std::mem::take(&mut self.stderr_pending);
        if !stdout.is_empty() {
            self.write_log_line(&stdout);
        }
        if !stderr.is_empty() {
            self.write_log_line(&stderr);
        }
    }

    fn accept_output(&mut self, stream: ProcessOutputStream, bytes: &[u8]) {
        if self.io_failed {
            return;
        }

        let mut pending = match stream {
            ProcessOutputStream::Stdout => std::mem::take(&mut self.stdout_pending),
            ProcessOutputStream::Stderr => std::mem::take(&mut self.stderr_pending),
        };
        pending.push_str(&sanitize_terminal_text(bytes));
        if let Some(percent) = extract_percentage(pending.as_bytes()) {
            if let Some(active) = &mut self.active {
                active.percent = Some(
                    active
                        .percent
                        .map_or(percent, |current| current.max(percent)),
                );
            }
        }
        let mut complete_lines = Vec::new();
        while let Some(newline) = pending.find('\n') {
            complete_lines.push(pending[..newline].to_owned());
            pending.drain(..=newline);
        }
        if pending.len() >= MAX_PENDING_OUTPUT {
            complete_lines.push(std::mem::take(&mut pending));
        }
        match stream {
            ProcessOutputStream::Stdout => self.stdout_pending = pending,
            ProcessOutputStream::Stderr => self.stderr_pending = pending,
        }
        for line in complete_lines {
            self.write_log_line(&line);
        }
        self.draw_footer();
    }

    fn update_elapsed(&mut self, elapsed: Duration) {
        if let Some(active) = &mut self.active {
            active.elapsed = elapsed;
            self.draw_footer();
        }
    }

    fn write_raw(&mut self, value: &str) {
        if self.io_failed {
            return;
        }
        let result = (|| -> io::Result<()> {
            let mut stdout = io::stdout().lock();
            stdout.write_all(value.as_bytes())?;
            stdout.flush()
        })();
        if result.is_err() {
            self.io_failed = true;
            self.footer_visible = false;
        }
    }
}

impl ExecutionObserver for MaintenanceTui {
    fn observe(&mut self, _plan: &ExecutionPlan, event: ProcessEvent) {
        match event {
            ProcessEvent::Output { stream, bytes } => self.accept_output(stream, &bytes),
            ProcessEvent::Tick { elapsed } | ProcessEvent::Heartbeat { elapsed } => {
                self.update_elapsed(elapsed)
            }
        }
    }

    fn handles_output(&self) -> bool {
        !self.io_failed
    }
}

fn terminal_width() -> usize {
    detected_terminal_width()
        .or_else(|| {
            env::var("COLUMNS")
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
        })
        .unwrap_or(DEFAULT_WIDTH)
        .clamp(MIN_WIDTH, MAX_WIDTH)
}

#[cfg(unix)]
fn detected_terminal_width() -> Option<usize> {
    use std::os::fd::AsRawFd;

    let mut size = unsafe { std::mem::zeroed::<libc::winsize>() };
    let result = unsafe { libc::ioctl(io::stdout().as_raw_fd(), libc::TIOCGWINSZ, &mut size) };
    (result == 0 && size.ws_col > 0).then_some(usize::from(size.ws_col))
}

#[cfg(not(unix))]
fn detected_terminal_width() -> Option<usize> {
    None
}

fn extract_percentage(bytes: &[u8]) -> Option<u8> {
    let mut latest = None;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'%' {
            continue;
        }
        let mut start = index;
        while start > 0 && bytes[start - 1].is_ascii_digit() && index - start < 3 {
            start -= 1;
        }
        if start == index || (start > 0 && bytes[start - 1].is_ascii_digit()) {
            continue;
        }
        if let Ok(value) = std::str::from_utf8(&bytes[start..index])
            .unwrap_or("")
            .parse::<u8>()
        {
            if value <= 100 {
                latest = Some(value);
            }
        }
    }
    latest
}

fn sanitize_terminal_text(bytes: &[u8]) -> String {
    let mut output = String::new();
    let decoded = String::from_utf8_lossy(bytes);
    let mut chars = decoded.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '\u{1b}' {
            match chars.next() {
                Some('[') => {
                    for next in chars.by_ref() {
                        if ('@'..='~').contains(&next) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    let mut previous_escape = false;
                    for next in chars.by_ref() {
                        if next == '\u{7}' || (previous_escape && next == '\\') {
                            break;
                        }
                        previous_escape = next == '\u{1b}';
                    }
                }
                Some(_) | None => {}
            }
            continue;
        }
        match character {
            '\n' => output.push('\n'),
            '\t' => output.push('\t'),
            '\r' if chars.peek() == Some(&'\n') => {}
            '\r' => output.push('\n'),
            value if value.is_control() => {}
            value => output.push(value),
        }
    }
    output
}

fn truncate(value: &str, max_width: usize) -> String {
    if display_width(value) <= max_width {
        return value.to_owned();
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width == 1 {
        return "…".to_owned();
    }
    let mut output = value.chars().take(max_width - 1).collect::<String>();
    output.push('…');
    output
}

fn display_width(value: &str) -> usize {
    value.chars().count()
}

fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds < 60 {
        return format!("{seconds}s");
    }
    format!("{}m {}s", seconds / 60, seconds % 60)
}

#[cfg(test)]
mod tests {
    use super::{extract_percentage, sanitize_terminal_text, truncate};

    #[test]
    fn terminal_projection_removes_control_sequences_but_keeps_text() {
        let output = sanitize_terminal_text(b"ok\x1b[31m red\x1b[0m\r\n\x1b]0;title\x07done\x08");

        assert_eq!(output, "ok red\ndone");
    }

    #[test]
    fn terminal_projection_turns_carriage_returns_into_safe_lines() {
        let output = sanitize_terminal_text(b"first\rsecond\r\nthird");

        assert_eq!(output, "first\nsecond\nthird");
    }

    #[test]
    fn apt_style_percentages_are_detected_from_streamed_output() {
        assert_eq!(extract_percentage(b"Progress: [ 42%]"), Some(42));
        assert_eq!(extract_percentage(b"download 8% then 100%"), Some(100));
        assert_eq!(extract_percentage(b"version 1200%"), None);
        assert_eq!(extract_percentage(b"no percentage"), None);
    }

    #[test]
    fn truncation_never_exceeds_terminal_width() {
        assert_eq!(truncate("a deliberately long footer", 8), "a delib…");
        assert_eq!(truncate("value", 0), "");
    }
}
