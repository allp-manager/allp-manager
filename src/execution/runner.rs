use crate::{
    domain::{AllpError, AllpResult, ExecutionPlan, NativeCommand, PrivilegeRequirement},
    execution::privilege::{prepare_command, UserAccount, UserContextExecutor},
    execution::render_native_command,
};
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
use std::{
    io::{IsTerminal, Read, Write},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const DEFAULT_CAPTURE_TIMEOUT: Duration = Duration::from_secs(15);
const FIRST_HEARTBEAT_AFTER: Duration = Duration::from_secs(12);
const REPEAT_HEARTBEAT_AFTER: Duration = Duration::from_secs(15);
const OUTPUT_DRAIN_GRACE: Duration = Duration::from_millis(750);
const MAX_CAPTURE_BYTES_PER_STREAM: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub success: bool,
    pub code: Option<i32>,
    pub signal: Option<i32>,
    pub duration: Duration,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone)]
pub struct ProcessStatus {
    pub success: bool,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

pub trait ProcessRunner: Send + Sync {
    fn capture(&self, command: &NativeCommand) -> AllpResult<CommandOutput>;
    fn capture_with_privilege(
        &self,
        command: &NativeCommand,
        _privilege: PrivilegeRequirement,
    ) -> AllpResult<CommandOutput> {
        self.capture(command)
    }
    fn capture_in_user_context(
        &self,
        command: &NativeCommand,
        _user: &UserAccount,
    ) -> AllpResult<CommandOutput> {
        self.capture_with_privilege(command, PrivilegeRequirement::OriginalUserRequired)
    }
    fn execute(&self, plan: &ExecutionPlan) -> AllpResult<ProcessStatus>;
}

#[derive(Debug, Default)]
pub struct StdProcessRunner;

impl StdProcessRunner {
    fn capture_prepared(
        &self,
        command: &NativeCommand,
        mut process: Command,
    ) -> AllpResult<CommandOutput> {
        let mut process = process;

        process
            .env("LC_ALL", "C")
            .env("LANG", "C")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(unix)]
        process.process_group(0);

        let mut child = process.spawn()?;
        let stdout = child.stdout.take().ok_or_else(|| {
            AllpError::Io(std::io::Error::other("failed to capture child stdout"))
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            AllpError::Io(std::io::Error::other("failed to capture child stderr"))
        })?;
        let (sender, receiver) = mpsc::channel();
        let _stdout_reader = read_stream(stdout, StreamKind::Stdout, sender.clone());
        let _stderr_reader = read_stream(stderr, StreamKind::Stderr, sender);
        let timeout = command.timeout.unwrap_or(DEFAULT_CAPTURE_TIMEOUT);
        let started = Instant::now();
        let mut captured_stdout = Vec::new();
        let mut captured_stderr = Vec::new();

        let status = loop {
            while let Ok(event) = receiver.try_recv() {
                capture_event(event, &mut captured_stdout, &mut captured_stderr);
            }
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if started.elapsed() >= timeout {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AllpError::Timeout(format!(
                    "Native command timed out after {} second(s): {}",
                    timeout.as_secs(),
                    render_native_command(command)
                )));
            }
            thread::sleep(Duration::from_millis(20));
        };

        let (stdout_tail, stderr_tail) = drain_output(&receiver, OUTPUT_DRAIN_GRACE, false)?;
        append_bounded(&mut captured_stdout, stdout_tail.as_bytes());
        append_bounded(&mut captured_stderr, stderr_tail.as_bytes());
        let stdout = String::from_utf8_lossy(&captured_stdout).into_owned();
        let stderr = String::from_utf8_lossy(&captured_stderr).into_owned();

        Ok(CommandOutput {
            success: status.success(),
            code: status.code(),
            signal: status_signal(&status),
            duration: started.elapsed(),
            stdout,
            stderr,
        })
    }
}

impl ProcessRunner for StdProcessRunner {
    fn capture(&self, command: &NativeCommand) -> AllpResult<CommandOutput> {
        self.capture_with_privilege(command, PrivilegeRequirement::NoElevation)
    }

    fn capture_with_privilege(
        &self,
        command: &NativeCommand,
        privilege: PrivilegeRequirement,
    ) -> AllpResult<CommandOutput> {
        let process = prepare_command(command, privilege)?;
        self.capture_prepared(command, process)
    }

    fn capture_in_user_context(
        &self,
        command: &NativeCommand,
        user: &UserAccount,
    ) -> AllpResult<CommandOutput> {
        let process = UserContextExecutor::prepare(command, user)?;
        self.capture_prepared(command, process)
    }

    fn execute(&self, plan: &ExecutionPlan) -> AllpResult<ProcessStatus> {
        let mut process = prepare_command(&plan.command, plan.privilege)?;
        let mut child = process
            .stdin(Stdio::inherit())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdout = child.stdout.take().ok_or_else(|| {
            AllpError::Io(std::io::Error::other("failed to capture child stdout"))
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            AllpError::Io(std::io::Error::other("failed to capture child stderr"))
        })?;
        let (sender, receiver) = mpsc::channel();
        let _stdout_reader = read_stream(stdout, StreamKind::Stdout, sender.clone());
        let _stderr_reader = read_stream(stderr, StreamKind::Stderr, sender);
        let started = Instant::now();
        let mut last_output = started;
        let mut next_heartbeat = FIRST_HEARTBEAT_AFTER;
        let heartbeat_enabled = std::io::stderr().is_terminal();
        let mut captured_stdout = Vec::new();
        let mut captured_stderr = Vec::new();

        let status = loop {
            while let Ok(event) = receiver.try_recv() {
                if let StreamEvent::Data(event) = event {
                    last_output = Instant::now();
                    let capture = match event.kind {
                        StreamKind::Stdout => &mut captured_stdout,
                        StreamKind::Stderr => &mut captured_stderr,
                    };
                    append_bounded(capture, &event.bytes);
                    write_stream_event(event)?;
                }
            }
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if heartbeat_enabled && last_output.elapsed() >= next_heartbeat {
                eprintln!(
                    "ℹ {} is still running · {} elapsed",
                    plan.backend_name,
                    format_elapsed(started.elapsed())
                );
                next_heartbeat = REPEAT_HEARTBEAT_AFTER;
                last_output = Instant::now();
            }
            thread::sleep(Duration::from_millis(25));
        };

        // The direct child status is authoritative. A detached descendant may
        // inherit these descriptors, so EOF must never own command completion.
        let (stdout_tail, stderr_tail) = drain_output(&receiver, OUTPUT_DRAIN_GRACE, true)?;
        append_bounded(&mut captured_stdout, stdout_tail.as_bytes());
        append_bounded(&mut captured_stderr, stderr_tail.as_bytes());
        let stdout = String::from_utf8_lossy(&captured_stdout).into_owned();
        let stderr = String::from_utf8_lossy(&captured_stderr).into_owned();

        Ok(ProcessStatus {
            success: status.success(),
            code: status.code(),
            stdout,
            stderr,
        })
    }
}

#[cfg(unix)]
fn status_signal(status: &std::process::ExitStatus) -> Option<i32> {
    status.signal()
}

#[cfg(not(unix))]
fn status_signal(_status: &std::process::ExitStatus) -> Option<i32> {
    None
}

#[derive(Debug, Clone, Copy)]
enum StreamKind {
    Stdout,
    Stderr,
}

#[derive(Debug)]
struct StreamData {
    kind: StreamKind,
    bytes: Vec<u8>,
}

#[derive(Debug)]
enum StreamEvent {
    Data(StreamData),
    Closed(StreamKind),
}

fn read_stream<R>(
    mut pipe: R,
    kind: StreamKind,
    sender: mpsc::Sender<StreamEvent>,
) -> thread::JoinHandle<()>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        loop {
            match pipe.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    let bytes = buffer[..count].to_vec();
                    if sender
                        .send(StreamEvent::Data(StreamData { kind, bytes }))
                        .is_err()
                    {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = sender.send(StreamEvent::Closed(kind));
    })
}

fn write_stream_event(event: StreamData) -> AllpResult<()> {
    match event.kind {
        StreamKind::Stdout => {
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(&event.bytes)?;
            stdout.flush()?;
        }
        StreamKind::Stderr => {
            let mut stderr = std::io::stderr().lock();
            stderr.write_all(&event.bytes)?;
            stderr.flush()?;
        }
    }
    Ok(())
}

fn drain_output(
    receiver: &mpsc::Receiver<StreamEvent>,
    grace: Duration,
    render: bool,
) -> AllpResult<(String, String)> {
    let deadline = Instant::now() + grace;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut stdout_closed = false;
    let mut stderr_closed = false;

    while !(stdout_closed && stderr_closed) {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        match receiver.recv_timeout(deadline.saturating_duration_since(now)) {
            Ok(StreamEvent::Data(event)) => {
                if render {
                    write_stream_event(StreamData {
                        kind: event.kind,
                        bytes: event.bytes.clone(),
                    })?;
                }
                let capture = match event.kind {
                    StreamKind::Stdout => &mut stdout,
                    StreamKind::Stderr => &mut stderr,
                };
                append_bounded(capture, &event.bytes);
            }
            Ok(StreamEvent::Closed(StreamKind::Stdout)) => stdout_closed = true,
            Ok(StreamEvent::Closed(StreamKind::Stderr)) => stderr_closed = true,
            Err(mpsc::RecvTimeoutError::Timeout | mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    Ok((
        String::from_utf8_lossy(&stdout).into_owned(),
        String::from_utf8_lossy(&stderr).into_owned(),
    ))
}

fn capture_event(event: StreamEvent, stdout: &mut Vec<u8>, stderr: &mut Vec<u8>) {
    if let StreamEvent::Data(event) = event {
        append_bounded(
            match event.kind {
                StreamKind::Stdout => stdout,
                StreamKind::Stderr => stderr,
            },
            &event.bytes,
        );
    }
}

fn append_bounded(output: &mut Vec<u8>, bytes: &[u8]) {
    let remaining = MAX_CAPTURE_BYTES_PER_STREAM.saturating_sub(output.len());
    output.extend_from_slice(&bytes[..bytes.len().min(remaining)]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn capture_finishes_when_detached_descendant_holds_pipes() {
        let runner = StdProcessRunner;
        let command = NativeCommand::new("/bin/sh").args([
            "-c",
            "printf 'main process started\\n'; (sleep 30) & exit 0",
        ]);
        let started = Instant::now();

        let output = runner.capture(&command).expect("command should succeed");

        assert!(output.success);
        assert!(output.stdout.contains("main process started"));
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[cfg(unix)]
    #[test]
    fn capture_drains_stdout_and_stderr_concurrently_with_bounded_diagnostics() {
        let runner = StdProcessRunner;
        let command = NativeCommand::new("/bin/sh").args([
            "-c",
            "(head -c 2097152 /dev/zero | tr '\\0' o) & (head -c 2097152 /dev/zero | tr '\\0' e >&2) & wait",
        ]);

        let output = runner.capture(&command).expect("command should succeed");

        assert!(output.success);
        assert_eq!(output.stdout.len(), MAX_CAPTURE_BYTES_PER_STREAM);
        assert_eq!(output.stderr.len(), MAX_CAPTURE_BYTES_PER_STREAM);
        assert!(output.stdout.bytes().all(|byte| byte == b'o'));
        assert!(output.stderr.bytes().all(|byte| byte == b'e'));
    }
}

fn format_elapsed(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        format!("{}m {}s", seconds / 60, seconds % 60)
    }
}
