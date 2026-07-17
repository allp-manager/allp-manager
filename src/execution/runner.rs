use crate::{
    domain::{AllpError, AllpResult, ExecutionPlan, NativeCommand, PrivilegeRequirement},
    execution::privilege::prepare_command,
    execution::render_native_command,
};
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
use std::{
    io::{IsTerminal, Read, Write},
    process::Stdio,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const DEFAULT_CAPTURE_TIMEOUT: Duration = Duration::from_secs(15);
const FIRST_HEARTBEAT_AFTER: Duration = Duration::from_secs(12);
const REPEAT_HEARTBEAT_AFTER: Duration = Duration::from_secs(15);

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
    fn execute(&self, plan: &ExecutionPlan) -> AllpResult<ProcessStatus>;
}

#[derive(Debug, Default)]
pub struct StdProcessRunner;

impl ProcessRunner for StdProcessRunner {
    fn capture(&self, command: &NativeCommand) -> AllpResult<CommandOutput> {
        let mut process = prepare_command(command, PrivilegeRequirement::NoElevation)?;
        let mut child = process
            .env("LC_ALL", "C")
            .env("LANG", "C")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdout = child.stdout.take().ok_or_else(|| {
            AllpError::Io(std::io::Error::other("failed to capture child stdout"))
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            AllpError::Io(std::io::Error::other("failed to capture child stderr"))
        })?;
        let stdout_reader = read_pipe(stdout);
        let stderr_reader = read_pipe(stderr);
        let timeout = command.timeout.unwrap_or(DEFAULT_CAPTURE_TIMEOUT);
        let started = Instant::now();

        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if started.elapsed() >= timeout {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(AllpError::Timeout(format!(
                    "Native command timed out after {} second(s): {}",
                    timeout.as_secs(),
                    render_native_command(command)
                )));
            }
            thread::sleep(Duration::from_millis(20));
        };

        let stdout = stdout_reader.join().unwrap_or_default();
        let stderr = stderr_reader.join().unwrap_or_default();

        Ok(CommandOutput {
            success: status.success(),
            code: status.code(),
            signal: status_signal(&status),
            duration: started.elapsed(),
            stdout,
            stderr,
        })
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
        let stdout_reader = read_stream(stdout, StreamKind::Stdout, sender.clone());
        let stderr_reader = read_stream(stderr, StreamKind::Stderr, sender);
        let started = Instant::now();
        let mut last_output = started;
        let mut next_heartbeat = FIRST_HEARTBEAT_AFTER;
        let heartbeat_enabled = std::io::stderr().is_terminal();

        let status = loop {
            while let Ok(event) = receiver.try_recv() {
                last_output = Instant::now();
                write_stream_event(event)?;
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

        let stdout = stdout_reader.join().unwrap_or_default();
        let stderr = stderr_reader.join().unwrap_or_default();
        while let Ok(event) = receiver.try_recv() {
            write_stream_event(event)?;
        }

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
struct StreamEvent {
    kind: StreamKind,
    bytes: Vec<u8>,
}

fn read_stream<R>(
    mut pipe: R,
    kind: StreamKind,
    sender: mpsc::Sender<StreamEvent>,
) -> thread::JoinHandle<String>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut output = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            match pipe.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    let bytes = buffer[..count].to_vec();
                    output.extend_from_slice(&bytes);
                    let _ = sender.send(StreamEvent { kind, bytes });
                }
                Err(_) => break,
            }
        }
        String::from_utf8_lossy(&output).into_owned()
    })
}

fn write_stream_event(event: StreamEvent) -> AllpResult<()> {
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

fn format_elapsed(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        format!("{}m {}s", seconds / 60, seconds % 60)
    }
}

fn read_pipe<R>(mut pipe: R) -> thread::JoinHandle<String>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut output = String::new();
        let _ = pipe.read_to_string(&mut output);
        output
    })
}
