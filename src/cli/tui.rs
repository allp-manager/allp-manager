use crate::{
    domain::{
        BackendOperationRecord, ExecutionPlan, MultiOperationReport, OperationStatus,
        RuntimePrivilegeContext,
    },
    execution::{
        render_execution_plan_with_context, ExecutionObserver, ProcessEvent, ProcessOutputStream,
    },
};
use std::{
    env,
    io::{self, Write},
    time::Duration,
};

const DEFAULT_WIDTH: usize = 88;
const MIN_WIDTH: usize = 48;
const MAX_WIDTH: usize = 112;
const FOOTER_BAR_WIDTH: usize = 18;
const MAX_LOG_LINE_WIDTH: usize = 220;

/// Inline live dashboard for maintenance operations.
///
/// It deliberately stays in the normal terminal buffer instead of taking over
/// the alternate screen. Native commands retain their inherited stdin, so an
/// unexpected package-manager prompt remains usable and a Ctrl-C cannot leave
/// the user's terminal in an alternate-screen state. The dashboard owns only
/// the current footer line; normal command output scrolls above it.
pub struct MaintenanceTui {
    operation: String,
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
    index: usize,
    backend_name: String,
    action: String,
    elapsed: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tone {
    Success,
    Warning,
    Error,
    Info,
    Muted,
}

impl MaintenanceTui {
    pub fn new(operation: &str, total: usize, color: bool) -> Self {
        let mut tui = Self {
            operation: operation.to_owned(),
            total,
            completed: 0,
            active: None,
            color,
            width: terminal_width(),
            stdout_pending: String::new(),
            stderr_pending: String::new(),
            footer_visible: false,
            io_failed: false,
        };
        tui.draw_header();
        tui.draw_footer();
        tui
    }

    pub fn start_operation(
        &mut self,
        index: usize,
        total: usize,
        plan: &ExecutionPlan,
        privilege_context: &RuntimePrivilegeContext,
    ) {
        self.total = total.max(1);
        self.flush_pending();
        self.begin_content();
        self.draw_card(
            &format!("RUNNING · {index}/{} · {}", self.total, plan.backend_name),
            Tone::Info,
            &[
                format!("Action: {}", plan.action),
                format!(
                    "Execution context: {}",
                    plan.privilege.label(privilege_context)
                ),
                format!(
                    "Command preview: {}",
                    render_execution_plan_with_context(plan, privilege_context)
                ),
                "The runtime preserves the validated privilege boundary and sanitized environment."
                    .to_owned(),
            ],
        );
        self.active = Some(ActiveOperation {
            index,
            backend_name: plan.backend_name.clone(),
            action: plan.action.clone(),
            elapsed: Duration::ZERO,
        });
        self.draw_footer();
    }

    pub fn finish_operation(
        &mut self,
        index: usize,
        total: usize,
        backend_name: &str,
        status: &OperationStatus,
        message: Option<&str>,
        elapsed: Duration,
    ) {
        self.total = total.max(1);
        self.flush_pending();
        self.begin_content();
        let mut lines = vec![format!("Result: {}", status.label())];
        if let Some(message) = message.filter(|message| !message.trim().is_empty()) {
            lines.push(message.to_owned());
        }
        lines.push(format!("Elapsed: {}", format_duration(elapsed)));
        self.draw_card(
            &format!(
                "{} · {index}/{} · {backend_name}",
                status.label().to_uppercase(),
                self.total
            ),
            tone_for_status(status),
            &lines,
        );
        self.completed = self.completed.max(index);
        self.active = None;
        self.draw_footer();
    }

    pub fn record_outcome(
        &mut self,
        index: usize,
        total: usize,
        backend_name: &str,
        status: &OperationStatus,
        message: Option<&str>,
    ) {
        self.total = total.max(1);
        self.flush_pending();
        self.begin_content();
        let mut lines = vec![format!("Result: {}", status.label())];
        if let Some(message) = message.filter(|message| !message.trim().is_empty()) {
            lines.push(message.to_owned());
        }
        self.draw_card(
            &format!(
                "{} · {index}/{} · {backend_name}",
                status.label().to_uppercase(),
                self.total
            ),
            tone_for_status(status),
            &lines,
        );
        self.completed = self.completed.max(index);
        self.active = None;
        self.draw_footer();
    }

    pub fn set_total(&mut self, total: usize) {
        self.total = total.max(1);
        self.draw_footer();
    }

    /// Clears the live footer before a native confirmation prompt is printed.
    ///
    /// The dashboard intentionally does not take over stdin; package-manager
    /// and follow-up confirmations therefore remain ordinary terminal prompts.
    pub fn prepare_for_prompt(&mut self) {
        self.flush_pending();
        self.begin_content();
    }

    /// Restores the progress footer after a native confirmation prompt.
    pub fn resume_after_prompt(&mut self) {
        self.draw_footer();
    }

    pub fn queue_extended(&mut self, total: usize, backend_name: &str, added: usize) {
        self.total = total.max(1);
        self.flush_pending();
        self.begin_content();
        self.draw_card(
            "QUEUE EXTENDED",
            Tone::Info,
            &[
                format!(
                    "{backend_name} added {added} follow-up operation(s) after metadata refresh."
                ),
                format!("Live progress now tracks {} operation(s).", self.total),
            ],
        );
        self.draw_footer();
    }

    /// Renders plans discovered after an earlier maintenance operation.
    ///
    /// A metadata refresh may reveal an upgrade plan only after it finishes.
    /// Show that exact plan before asking the user for the separate follow-up
    /// confirmation, preserving the same review boundary as the initial queue.
    pub fn show_follow_up_plans(
        &mut self,
        plans: &[ExecutionPlan],
        privilege_context: &RuntimePrivilegeContext,
    ) {
        self.flush_pending();
        self.begin_content();
        for (position, plan) in plans.iter().enumerate() {
            self.draw_card(
                &format!(
                    "FOLLOW-UP PLAN · {}/{} · {}",
                    position + 1,
                    plans.len(),
                    plan.backend_name
                ),
                Tone::Info,
                &[
                    format!("Action: {}", plan.action),
                    format!(
                        "Execution context: {}",
                        plan.privilege.label(privilege_context)
                    ),
                    format!(
                        "Command preview: {}",
                        render_execution_plan_with_context(plan, privilege_context)
                    ),
                ],
            );
        }
        self.draw_footer();
    }

    pub fn finish(&mut self, report: &MultiOperationReport, verbose: bool, dry_run: bool) {
        self.flush_pending();
        self.begin_content();
        self.draw_card(
            &format!("{} SUMMARY", self.operation.to_uppercase()),
            if report.has_failures() {
                Tone::Warning
            } else {
                Tone::Success
            },
            &summary_lines(report, dry_run),
        );

        for record in visible_records(report, verbose) {
            self.draw_record_card(record);
        }
        self.write_raw("\n");
    }

    fn draw_header(&mut self) {
        self.begin_content();
        self.draw_card(
            &format!("ALLP · {} · LIVE", self.operation.to_uppercase()),
            Tone::Info,
            &[
                "Native output is streamed below without changing the command being run."
                    .to_owned(),
                "The footer tracks the active backend, exact action, elapsed time, and queue progress."
                    .to_owned(),
                "Use --no-tui for the classic streaming view.".to_owned(),
            ],
        );
    }

    fn draw_record_card(&mut self, record: &BackendOperationRecord) {
        let mut lines = Vec::new();
        if let Some(action) = &record.action {
            lines.push(format!("Action: {action}"));
        }
        if let Some(message) = &record.message {
            lines.push(message.clone());
        }
        if lines.is_empty() {
            lines.push("No additional detail reported.".to_owned());
        }
        self.draw_card(
            &format!(
                "{} · {}",
                record.status.label().to_uppercase(),
                record.backend_name
            ),
            tone_for_status(&record.status),
            &lines,
        );
    }

    fn draw_card(&mut self, title: &str, tone: Tone, lines: &[String]) {
        let inner_width = self.width.saturating_sub(4).max(1);
        let title = truncate(
            &sanitize_terminal_text(title.as_bytes()),
            inner_width.saturating_sub(2),
        );
        let fill = inner_width
            .saturating_sub(display_width(&title))
            .saturating_sub(1);
        self.write_line(&self.styled(&format!("╭─ {title} {}", "─".repeat(fill)), tone));
        for line in lines {
            let safe = sanitize_terminal_text(line.as_bytes());
            let wrapped = wrap_line(&safe, inner_width);
            for line in wrapped {
                let padded = pad_to_width(&line, inner_width);
                self.write_line(&self.styled(&format!("│ {padded} │"), tone));
            }
        }
        self.write_line(&self.styled(&format!("╰{}╯", "─".repeat(inner_width + 2)), tone));
    }

    fn draw_footer(&mut self) {
        if self.io_failed {
            return;
        }
        let total = self.total.max(1);
        let (index, backend, action, elapsed) = self
            .active
            .as_ref()
            .map(|active| {
                (
                    active.index,
                    active.backend_name.as_str(),
                    active.action.as_str(),
                    active.elapsed,
                )
            })
            .unwrap_or((
                self.completed.min(total),
                "waiting",
                "finalizing results",
                Duration::ZERO,
            ));
        let bar = progress_bar(self.completed, self.active.is_some(), total, elapsed);
        let text = format!(
            " {bar} {index}/{total} · {backend} · {} · {} ",
            truncate(action, 34),
            format_duration(elapsed)
        );
        self.write_raw("\r\x1b[2K");
        self.write_raw(&self.styled(&text, Tone::Info));
        self.footer_visible = !self.io_failed;
    }

    fn begin_content(&mut self) {
        if self.footer_visible {
            self.write_raw("\r\x1b[2K\n");
            self.footer_visible = false;
        }
    }

    fn write_line(&mut self, value: &str) {
        self.write_raw(value);
        self.write_raw("\n");
    }

    fn write_log_line(&mut self, stream: ProcessOutputStream, line: &str) {
        let line = truncate(line.trim_end(), MAX_LOG_LINE_WIDTH);
        if line.trim().is_empty() {
            return;
        }
        self.begin_content();
        let (marker, tone) = match stream {
            ProcessOutputStream::Stdout => ("›", Tone::Muted),
            ProcessOutputStream::Stderr => ("!", Tone::Warning),
        };
        let prefix = self.styled(&format!("{marker} "), tone);
        self.write_line(&format!("{prefix}{line}"));
        self.draw_footer();
    }

    fn flush_pending(&mut self) {
        let stdout = std::mem::take(&mut self.stdout_pending);
        let stderr = std::mem::take(&mut self.stderr_pending);
        if !stdout.trim().is_empty() {
            self.write_log_line(ProcessOutputStream::Stdout, &stdout);
        }
        if !stderr.trim().is_empty() {
            self.write_log_line(ProcessOutputStream::Stderr, &stderr);
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
        let mut complete_lines = Vec::new();
        while let Some(newline) = pending.find('\n') {
            complete_lines.push(pending[..newline].to_owned());
            pending.drain(..=newline);
        }
        if display_width(&pending) > MAX_LOG_LINE_WIDTH {
            complete_lines.push(std::mem::take(&mut pending));
        }
        match stream {
            ProcessOutputStream::Stdout => self.stdout_pending = pending,
            ProcessOutputStream::Stderr => self.stderr_pending = pending,
        }
        for line in complete_lines {
            self.write_log_line(stream, &line);
        }
        self.draw_footer();
    }

    fn update_elapsed(&mut self, elapsed: Duration, heartbeat: bool) {
        // Some package managers report progress without a trailing newline.
        // Surface that partial line on the next live tick instead of hiding it
        // until the child exits.
        self.flush_pending();
        let backend_name = if let Some(active) = &mut self.active {
            active.elapsed = elapsed;
            active.backend_name.clone()
        } else {
            return;
        };
        if heartbeat {
            self.begin_content();
            self.write_line(&format!(
                "{} {} is still running · {} elapsed",
                self.styled("ℹ", Tone::Info),
                backend_name,
                format_duration(elapsed)
            ));
        }
        self.draw_footer();
    }

    fn styled(&self, value: &str, tone: Tone) -> String {
        if !self.color {
            return value.to_owned();
        }
        let code = match tone {
            Tone::Success => "32",
            Tone::Warning => "33",
            Tone::Error => "31",
            Tone::Info => "36",
            Tone::Muted => "2",
        };
        format!("\x1b[{code}m{value}\x1b[0m")
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
            ProcessEvent::Tick { elapsed } => self.update_elapsed(elapsed, false),
            ProcessEvent::Heartbeat { elapsed } => self.update_elapsed(elapsed, true),
        }
    }

    fn handles_output(&self) -> bool {
        !self.io_failed
    }
}

fn terminal_width() -> usize {
    env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(DEFAULT_WIDTH)
        .clamp(MIN_WIDTH, MAX_WIDTH)
}

fn tone_for_status(status: &OperationStatus) -> Tone {
    match status {
        OperationStatus::Updated
        | OperationStatus::UpToDate
        | OperationStatus::Completed
        | OperationStatus::AlreadyInstalled
        | OperationStatus::Success => Tone::Success,
        OperationStatus::Failed => Tone::Error,
        OperationStatus::Protected | OperationStatus::Busy | OperationStatus::Deferred => {
            Tone::Warning
        }
        OperationStatus::DryRun | OperationStatus::Available | OperationStatus::Selected => {
            Tone::Info
        }
        OperationStatus::NotApplicable
        | OperationStatus::NotSelected
        | OperationStatus::Unavailable
        | OperationStatus::Cancelled
        | OperationStatus::Skipped => Tone::Muted,
    }
}

fn visible_records(
    report: &MultiOperationReport,
    verbose: bool,
) -> impl Iterator<Item = &BackendOperationRecord> {
    report
        .records
        .iter()
        .filter(move |record| verbose || !record.status.is_optional_unavailable())
}

fn summary_lines(report: &MultiOperationReport, dry_run: bool) -> Vec<String> {
    if dry_run {
        let planned = report
            .records
            .iter()
            .filter(|record| record.command.is_some())
            .count();
        return vec![
            "Dry run completed; no native command was executed.".to_owned(),
            format!("{planned} operation(s) planned"),
        ];
    }
    let count = |predicate: fn(&OperationStatus) -> bool| {
        report
            .records
            .iter()
            .filter(|record| predicate(&record.status))
            .count()
    };
    vec![
        format!(
            "{} completed · {} updated · {} up to date",
            count(|status| matches!(
                status,
                OperationStatus::Completed | OperationStatus::Success
            )),
            count(|status| matches!(status, OperationStatus::Updated)),
            count(|status| matches!(status, OperationStatus::UpToDate)),
        ),
        format!(
            "{} deferred · {} not applicable · {} protected · {} busy · {} cancelled · {} failed",
            count(|status| matches!(status, OperationStatus::Deferred)),
            count(|status| matches!(status, OperationStatus::NotApplicable)),
            count(|status| matches!(status, OperationStatus::Protected)),
            count(|status| matches!(status, OperationStatus::Busy)),
            count(|status| matches!(status, OperationStatus::Cancelled)),
            count(|status| matches!(status, OperationStatus::Failed)),
        ),
    ]
}

fn progress_bar(completed: usize, active: bool, total: usize, elapsed: Duration) -> String {
    let total = total.max(1);
    let settled_width = completed.min(total) * FOOTER_BAR_WIDTH / total;
    let pulse_width = if active && settled_width < FOOTER_BAR_WIDTH {
        1 + (elapsed.as_secs() as usize % 3)
    } else {
        0
    };
    let filled = (settled_width + pulse_width).min(FOOTER_BAR_WIDTH);
    format!(
        "[{}{}]",
        "█".repeat(filled),
        "░".repeat(FOOTER_BAR_WIDTH - filled)
    )
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
                        if next == '\u{7}' {
                            break;
                        }
                        if previous_escape && next == '\\' {
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
            '\t' => output.push_str("  "),
            '\r' if chars.peek() == Some(&'\n') => {}
            '\r' => output.push('\n'),
            value if value.is_control() => {}
            value => output.push(value),
        }
    }
    output
}

fn wrap_line(value: &str, width: usize) -> Vec<String> {
    if value.is_empty() {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in value.split_whitespace() {
        let separator = if current.is_empty() { "" } else { " " };
        if display_width(&current) + separator.len() + display_width(word) > width
            && !current.is_empty()
        {
            lines.push(std::mem::take(&mut current));
        }
        if display_width(word) > width {
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            lines.push(truncate(word, width));
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn truncate(value: &str, max_width: usize) -> String {
    if display_width(value) <= max_width {
        return value.to_owned();
    }
    if max_width <= 1 {
        return "…".chars().take(max_width).collect();
    }
    let mut output = String::new();
    for character in value.chars().take(max_width - 1) {
        output.push(character);
    }
    output.push('…');
    output
}

fn display_width(value: &str) -> usize {
    value.chars().count()
}

fn pad_to_width(value: &str, width: usize) -> String {
    format!(
        "{value}{}",
        " ".repeat(width.saturating_sub(display_width(value)))
    )
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
    use super::{progress_bar, sanitize_terminal_text, summary_lines, wrap_line};
    use crate::domain::{BackendOperationRecord, MultiOperationReport, OperationStatus};
    use std::time::Duration;

    #[test]
    fn terminal_projection_removes_control_sequences_but_keeps_text() {
        let output = sanitize_terminal_text(b"ok\x1b[31m red\x1b[0m\r\n\x1b]0;title\x07done\x08");

        assert_eq!(output, "ok red\ndone");
    }

    #[test]
    fn terminal_projection_turns_progress_carriage_returns_into_safe_lines() {
        let output = sanitize_terminal_text(b"first\rsecond\r\nthird");

        assert_eq!(output, "first\nsecond\nthird");
    }

    #[test]
    fn progress_bar_handles_a_growing_queue() {
        let before_follow_up = progress_bar(1, true, 2, Duration::from_secs(4));
        let after_follow_up = progress_bar(1, true, 3, Duration::from_secs(4));

        assert!(before_follow_up.contains('█'));
        assert!(after_follow_up.contains('░'));
        assert_ne!(before_follow_up, after_follow_up);
    }

    #[test]
    fn summary_keeps_failure_and_protected_counts_separate() {
        let report = MultiOperationReport {
            operation: "update".to_owned(),
            records: vec![
                record(OperationStatus::Completed),
                record(OperationStatus::Protected),
                record(OperationStatus::Failed),
            ],
        };
        let summary = summary_lines(&report, false).join("\n");

        assert!(summary.contains("1 completed"));
        assert!(summary.contains("1 protected"));
        assert!(summary.contains("1 failed"));
    }

    #[test]
    fn wrapped_lines_never_exceed_the_requested_width() {
        let lines = wrap_line("a deliberately long line for terminal cards", 12);

        assert!(lines.iter().all(|line| line.chars().count() <= 12));
    }

    fn record(status: OperationStatus) -> BackendOperationRecord {
        BackendOperationRecord {
            backend_id: "test".to_owned(),
            backend_name: "Test".to_owned(),
            action: None,
            command: None,
            status,
            message: None,
        }
    }
}
