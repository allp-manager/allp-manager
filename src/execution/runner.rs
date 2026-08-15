use crate::{
    domain::{
        AllpError, AllpResult, ExecutionPlan, NativeCommand, PrivilegeRequirement, PrivilegeStatus,
    },
    execution::privilege::{
        prepare_command, prepare_command_with_privilege_session, PrivilegeSession, UserAccount,
        UserContextExecutor,
    },
    execution::render_native_command,
};
#[cfg(unix)]
use std::os::unix::process::{Child, CommandExt, ExitStatusExt};
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
const TUI_TICK_INTERVAL: Duration = Duration::from_secs(1);
const PROCESS_TERMINATION_GRACE: Duration = Duration::from_millis(500);

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

/// The result of attempting one plan through a privilege session.
///
/// `PrivilegeBlocked` is emitted before the native child is spawned: when the
/// session is unavailable or the already-validated sudo helper itself cannot
/// start. A nonzero child status remains a native result; stdout/stderr text
/// alone cannot prove whether sudo reached the native executable.
#[derive(Debug, Clone)]
pub enum ProcessExecutionOutcome {
    Process(ProcessStatus),
    PrivilegeBlocked(PrivilegeStatus),
}

/// Origin of native output observed while an execution plan is running.
///
/// The central runner remains responsible for spawning and privilege handling;
/// observers are presentation-only and must never change the command being run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessOutputStream {
    Stdout,
    Stderr,
}

/// A presentation event emitted by the central process runner.
///
/// Output bytes remain available in the final `ProcessStatus` exactly as before.
/// A live UI may render a safe projection of those bytes while the command runs.
#[derive(Debug, Clone)]
pub enum ProcessEvent {
    Output {
        stream: ProcessOutputStream,
        bytes: Vec<u8>,
    },
    Tick {
        elapsed: Duration,
    },
    Heartbeat {
        elapsed: Duration,
    },
}

/// Receives live process events for a presentation layer such as the maintenance TUI.
///
/// Implementations must treat input as untrusted terminal data. When
/// `handles_output()` returns `false`, the runner resumes ordinary stdout/stderr
/// streaming, so a UI rendering failure never interrupts a package-manager mutation.
pub trait ExecutionObserver {
    fn observe(&mut self, plan: &ExecutionPlan, event: ProcessEvent);

    fn handles_output(&self) -> bool {
        true
    }
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

    fn execute_with_observer(
        &self,
        plan: &ExecutionPlan,
        _observer: &mut dyn ExecutionObserver,
    ) -> AllpResult<ProcessStatus> {
        self.execute(plan)
    }

    fn execute_with_privilege_session(
        &self,
        plan: &ExecutionPlan,
        session: &mut PrivilegeSession,
    ) -> AllpResult<ProcessExecutionOutcome> {
        if plan.privilege.requires_sudo(session.context()) {
            return Ok(ProcessExecutionOutcome::PrivilegeBlocked(
                PrivilegeStatus::Unavailable,
            ));
        }
        self.execute(plan).map(ProcessExecutionOutcome::Process)
    }

    fn execute_with_observer_and_privilege_session(
        &self,
        plan: &ExecutionPlan,
        session: &mut PrivilegeSession,
        observer: &mut dyn ExecutionObserver,
    ) -> AllpResult<ProcessExecutionOutcome> {
        if plan.privilege.requires_sudo(session.context()) {
            return Ok(ProcessExecutionOutcome::PrivilegeBlocked(
                PrivilegeStatus::Unavailable,
            ));
        }
        self.execute_with_observer(plan, observer)
            .map(ProcessExecutionOutcome::Process)
    }
}

/// Restricts backend maintenance hooks while a privilege session is active.
///
/// Hook APIs predate privilege sessions and expose legacy
/// `capture_with_privilege`, which could otherwise construct interactive
/// `sudo --`. The live maintenance path gives hooks this adapter instead of
/// the raw runner. Root-required hook work must be modelled as an execution
/// plan, where the session can validate it before it reaches the TUI.
pub struct MaintenanceHookRunner<'a> {
    inner: &'a dyn ProcessRunner,
    context: &'a crate::domain::RuntimePrivilegeContext,
}

impl<'a> MaintenanceHookRunner<'a> {
    pub fn new(inner: &'a dyn ProcessRunner, session: &'a PrivilegeSession) -> Self {
        Self {
            inner,
            context: session.context(),
        }
    }

    fn allow(&self, privilege: PrivilegeRequirement) -> AllpResult<()> {
        if privilege == PrivilegeRequirement::Conditional || privilege.requires_sudo(self.context) {
            return Err(AllpError::InvalidInput(
                "root-required maintenance hooks are forbidden while a privilege session is active; create an execution plan instead".to_owned(),
            ));
        }
        Ok(())
    }
}

impl ProcessRunner for MaintenanceHookRunner<'_> {
    fn capture(&self, command: &NativeCommand) -> AllpResult<CommandOutput> {
        self.inner.capture(command)
    }

    fn capture_with_privilege(
        &self,
        command: &NativeCommand,
        privilege: PrivilegeRequirement,
    ) -> AllpResult<CommandOutput> {
        self.allow(privilege)?;
        self.inner.capture_with_privilege(command, privilege)
    }

    fn capture_in_user_context(
        &self,
        command: &NativeCommand,
        user: &UserAccount,
    ) -> AllpResult<CommandOutput> {
        self.inner.capture_in_user_context(command, user)
    }

    fn execute(&self, plan: &ExecutionPlan) -> AllpResult<ProcessStatus> {
        self.allow(plan.privilege)?;
        self.inner.execute(plan)
    }

    fn execute_with_observer(
        &self,
        plan: &ExecutionPlan,
        observer: &mut dyn ExecutionObserver,
    ) -> AllpResult<ProcessStatus> {
        self.allow(plan.privilege)?;
        self.inner.execute_with_observer(plan, observer)
    }

    fn execute_with_privilege_session(
        &self,
        plan: &ExecutionPlan,
        session: &mut PrivilegeSession,
    ) -> AllpResult<ProcessExecutionOutcome> {
        self.allow(plan.privilege)?;
        self.inner.execute_with_privilege_session(plan, session)
    }

    fn execute_with_observer_and_privilege_session(
        &self,
        plan: &ExecutionPlan,
        session: &mut PrivilegeSession,
        observer: &mut dyn ExecutionObserver,
    ) -> AllpResult<ProcessExecutionOutcome> {
        self.allow(plan.privilege)?;
        self.inner
            .execute_with_observer_and_privilege_session(plan, session, observer)
    }
}

#[derive(Debug, Default)]
pub struct StdProcessRunner;

impl StdProcessRunner {
    fn capture_prepared(
        &self,
        command: &NativeCommand,
        process: Command,
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

        let mut observer = None;
        let (stdout_tail, stderr_tail) =
            drain_output(&receiver, OUTPUT_DRAIN_GRACE, false, None, &mut observer)?;
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
        self.execute_internal(plan, None)
    }

    fn execute_with_observer(
        &self,
        plan: &ExecutionPlan,
        observer: &mut dyn ExecutionObserver,
    ) -> AllpResult<ProcessStatus> {
        self.execute_internal(plan, Some(observer))
    }

    fn execute_with_privilege_session(
        &self,
        plan: &ExecutionPlan,
        session: &mut PrivilegeSession,
    ) -> AllpResult<ProcessExecutionOutcome> {
        self.execute_internal_with_privilege_session(plan, session, None)
    }

    fn execute_with_observer_and_privilege_session(
        &self,
        plan: &ExecutionPlan,
        session: &mut PrivilegeSession,
        observer: &mut dyn ExecutionObserver,
    ) -> AllpResult<ProcessExecutionOutcome> {
        self.execute_internal_with_privilege_session(plan, session, Some(observer))
    }
}

impl StdProcessRunner {
    fn execute_internal(
        &self,
        plan: &ExecutionPlan,
        observer: Option<&mut dyn ExecutionObserver>,
    ) -> AllpResult<ProcessStatus> {
        let process = prepare_command(&plan.command, plan.privilege)?;
        self.execute_prepared(plan, process, observer, true)
    }

    fn execute_internal_with_privilege_session(
        &self,
        plan: &ExecutionPlan,
        session: &mut PrivilegeSession,
        observer: Option<&mut dyn ExecutionObserver>,
    ) -> AllpResult<ProcessExecutionOutcome> {
        let privilege_status = session.current_status_for(plan);
        if !privilege_status.permits_execution() {
            return Ok(ProcessExecutionOutcome::PrivilegeBlocked(privilege_status));
        }

        let uses_sudo = plan.privilege.requires_sudo(session.context());
        let process =
            prepare_command_with_privilege_session(&plan.command, plan.privilege, session)?;
        // `sudo -n` cannot ask for a password, but it is still a wrapper.  It
        // can fail or linger before it execs the native program, and a wrapper
        // spawn is not evidence that APT/Snap/etc. has started.  Suppress
        // heartbeats in that narrow case rather than claiming backend progress
        // before we have a trusted native-start protocol.
        let status = match self.execute_prepared(plan, process, observer, !uses_sudo) {
            Ok(status) => status,
            Err(error) if uses_sudo && is_sudo_wrapper_spawn_error(&error) => {
                session.mark_noninteractive_failure(PrivilegeStatus::Unavailable);
                return Ok(ProcessExecutionOutcome::PrivilegeBlocked(
                    PrivilegeStatus::Unavailable,
                ));
            }
            Err(error) => return Err(error),
        };
        Ok(ProcessExecutionOutcome::Process(status))
    }

    fn execute_prepared(
        &self,
        plan: &ExecutionPlan,
        process: Command,
        observer: Option<&mut dyn ExecutionObserver>,
        heartbeat_allowed: bool,
    ) -> AllpResult<ProcessStatus> {
        let mut process = process;
        
        #[cfg(unix)]
        process.process_group(0);

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
        let timeout = plan
            .command
            .timeout
            .unwrap_or(DEFAULT_CAPTURE_TIMEOUT);
        let mut last_output = started;
        let mut next_heartbeat = FIRST_HEARTBEAT_AFTER;
        let heartbeat_enabled = heartbeat_allowed && std::io::stderr().is_terminal();
        let mut last_tick = started;
        let mut observer = observer;
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
                    render_stream_event(plan, &mut observer, event)?;
                }
            }
            if let Some(status) = child.try_wait()? {
                break status;
                
            }

            
            if started.elapsed() >= timeout {
                terminate_process_tree(&mut child)?;

                return Err(AllpError::Timeout(format!(
                    "Native command timed out after {} second(s): {}",
                    timeout.as_secs(),
                    render_native_command(&plan.command)
                )));
            }
            
            if last_tick.elapsed() >= TUI_TICK_INTERVAL {
                emit_progress_event(
                    plan,
                    &mut observer,
                    ProcessEvent::Tick {
                        elapsed: started.elapsed(),
                    },
                );
                last_tick = Instant::now();
            }
            if heartbeat_enabled && last_output.elapsed() >= next_heartbeat {
                let elapsed = started.elapsed();
                emit_progress_event(plan, &mut observer, ProcessEvent::Heartbeat { elapsed });
                next_heartbeat = REPEAT_HEARTBEAT_AFTER;
                last_output = Instant::now();
            }
            thread::sleep(Duration::from_millis(25));
        };

        // The direct child status is authoritative. A detached descendant may
        // inherit these descriptors, so EOF must never own command completion.
        let (stdout_tail, stderr_tail) = drain_output(
            &receiver,
            OUTPUT_DRAIN_GRACE,
            true,
            Some(plan),
            &mut observer,
        )?;
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

fn terminate_process_tree(child: &mut Child) -> AllpResult<()> {
    #[cfg(unix)]
    {
        use std::io;

        let pid = child.id();

        // The child is placed in its own process group.
        // A negative PID targets the entire process group.
        let result = unsafe { libc::kill(-(pid as libc::pid_t), libc::SIGTERM) };

        if result != 0 {
            let error = io::Error::last_os_error();

            // ESRCH means the process/group already exited.
            if error.raw_os_error() != Some(libc::ESRCH) {
                return Err(AllpError::Io(error));
            }
        }

        let deadline = Instant::now() + PROCESS_TERMINATION_GRACE;

        loop {
            if child.try_wait()?.is_some() {
                return Ok(());
            }

            if Instant::now() >= deadline {
                break;
            }

            thread::sleep(Duration::from_millis(25));
        }

        // Grace period expired. Kill the entire process group.
        let result = unsafe { libc::kill(-(pid as libc::pid_t), libc::SIGKILL) };

        if result != 0 {
            let error = io::Error::last_os_error();

            if error.raw_os_error() != Some(libc::ESRCH) {
                return Err(AllpError::Io(error));
            }
        }

        let _ = child.wait();

        return Ok(());
    }

    #[cfg(not(unix))]
    {
        child.kill()?;
        child.wait()?;
        Ok(())
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

#[cfg(unix)]
fn is_sudo_wrapper_spawn_error(error: &AllpError) -> bool {
    matches!(
        error,
        AllpError::Io(error)
            if matches!(
                error.raw_os_error(),
                Some(libc::ENOENT | libc::EACCES | libc::ENOTDIR | libc::EPERM)
            )
    )
}

#[cfg(not(unix))]
fn is_sudo_wrapper_spawn_error(_error: &AllpError) -> bool {
    false
}

#[derive(Debug, Clone, Copy)]
enum StreamKind {
    Stdout,
    Stderr,
}

impl From<StreamKind> for ProcessOutputStream {
    fn from(value: StreamKind) -> Self {
        match value {
            StreamKind::Stdout => Self::Stdout,
            StreamKind::Stderr => Self::Stderr,
        }
    }
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

fn write_stream_bytes(kind: StreamKind, bytes: &[u8]) -> AllpResult<()> {
    match kind {
        StreamKind::Stdout => {
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(bytes)?;
            stdout.flush()?;
        }
        StreamKind::Stderr => {
            let mut stderr = std::io::stderr().lock();
            stderr.write_all(bytes)?;
            stderr.flush()?;
        }
    }
    Ok(())
}

fn render_stream_event(
    plan: &ExecutionPlan,
    observer: &mut Option<&mut dyn ExecutionObserver>,
    event: StreamData,
) -> AllpResult<()> {
    if let Some(observer) = observer.as_deref_mut() {
        observer.observe(
            plan,
            ProcessEvent::Output {
                stream: event.kind.into(),
                bytes: event.bytes.clone(),
            },
        );
        if observer.handles_output() {
            return Ok(());
        }
        // The observer has explicitly relinquished the stream, normally because
        // presentation I/O failed. Do not turn that loss of presentation into a
        // failure of an already-running package-manager command. The final raw
        // ProcessStatus still retains this output for classification.
        let _ = write_stream_bytes(event.kind, &event.bytes);
        return Ok(());
    }
    write_stream_bytes(event.kind, &event.bytes)
}

fn emit_progress_event(
    plan: &ExecutionPlan,
    observer: &mut Option<&mut dyn ExecutionObserver>,
    event: ProcessEvent,
) {
    if let Some(observer) = observer.as_deref_mut() {
        observer.observe(plan, event.clone());
        if observer.handles_output() {
            return;
        }
    }
    if let ProcessEvent::Heartbeat { elapsed } = event {
        eprintln!(
            "ℹ {} is still running · {} elapsed",
            plan.backend_name,
            format_elapsed(elapsed)
        );
    }
}

fn drain_output(
    receiver: &mpsc::Receiver<StreamEvent>,
    grace: Duration,
    render: bool,
    plan: Option<&ExecutionPlan>,
    observer: &mut Option<&mut dyn ExecutionObserver>,
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
                    if let Some(plan) = plan {
                        render_stream_event(
                            plan,
                            observer,
                            StreamData {
                                kind: event.kind,
                                bytes: event.bytes.clone(),
                            },
                        )?;
                    } else {
                        write_stream_bytes(event.kind, &event.bytes)?;
                    }
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

fn format_elapsed(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        format!("{}m {}s", seconds / 60, seconds % 60)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    
    #[derive(Default)]
    struct RecordingObserver {
        events: Vec<ProcessEvent>,
    }

    #[derive(Default)]
    struct HookRecordingRunner {
        privileged_captures: AtomicUsize,
        executes: AtomicUsize,
    }

    #[test]
    fn execute_times_out_and_terminates_process_group() {
        let runner = StdProcessRunner;
    
        let mut command = NativeCommand::new("/bin/sh");
    
        command = command.args([
            "-c",
            "sleep 30",
        ]);
    
        command.timeout = Some(Duration::from_millis(250));
    
        let plan = ExecutionPlan {
            backend_id: "test".to_owned(),
            backend_name: "Test".to_owned(),
            operation: crate::domain::OperationKind::Update,
            action: "Timeout test".to_owned(),
            package_id: None,
            source: None,
            scope: None,
            details: Vec::new(),
            command,
            privilege: PrivilegeRequirement::NoElevation,
            requires_root: false,
            interactive: false,
        };
    
        let started = Instant::now();
    
        let result = runner.execute(&plan);
    
        assert!(matches!(result, Err(AllpError::Timeout(_))));
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn execute_timeout_terminates_descendants() {
        let runner = StdProcessRunner;
    
        let mut command = NativeCommand::new("/bin/sh");
    
        command = command.args([
            "-c",
            "sleep 30 & wait",
        ]);
    
        command.timeout = Some(Duration::from_millis(250));
    
        let plan = ExecutionPlan {
            backend_id: "test".to_owned(),
            backend_name: "Test".to_owned(),
            operation: crate::domain::OperationKind::Update,
            action: "Process group timeout test".to_owned(),
            package_id: None,
            source: None,
            scope: None,
            details: Vec::new(),
            command,
            privilege: PrivilegeRequirement::NoElevation,
            requires_root: false,
            interactive: false,
        };
    
        let started = Instant::now();
    
        let result = runner.execute(&plan);
    
        assert!(matches!(result, Err(AllpError::Timeout(_))));
        assert!(started.elapsed() < Duration::from_secs(3));
    }
    
    impl ProcessRunner for HookRecordingRunner {
        fn capture(&self, _command: &NativeCommand) -> AllpResult<CommandOutput> {
            Ok(CommandOutput {
                success: true,
                code: Some(0),
                signal: None,
                duration: Duration::ZERO,
                stdout: String::new(),
                stderr: String::new(),
            })
        }

        fn capture_with_privilege(
            &self,
            command: &NativeCommand,
            _privilege: PrivilegeRequirement,
        ) -> AllpResult<CommandOutput> {
            self.privileged_captures.fetch_add(1, Ordering::Relaxed);
            self.capture(command)
        }

        fn execute(&self, _plan: &ExecutionPlan) -> AllpResult<ProcessStatus> {
            self.executes.fetch_add(1, Ordering::Relaxed);
            Ok(ProcessStatus {
                success: true,
                code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }

    fn hook_plan(privilege: PrivilegeRequirement) -> ExecutionPlan {
        ExecutionPlan {
            backend_id: "test".to_owned(),
            backend_name: "Test".to_owned(),
            operation: crate::domain::OperationKind::Update,
            action: "Test hook".to_owned(),
            package_id: None,
            source: None,
            scope: None,
            details: Vec::new(),
            command: NativeCommand::new("/bin/true"),
            privilege,
            requires_root: privilege == PrivilegeRequirement::RootRequired,
            interactive: false,
        }
    }

    #[test]
    fn maintenance_hook_runner_rejects_sudo_capable_hooks_without_calling_inner_runner() {
        let inner = HookRecordingRunner::default();
        let root_plan = hook_plan(PrivilegeRequirement::RootRequired);
        let session = PrivilegeSession::for_plans(
            std::slice::from_ref(&root_plan),
            &crate::domain::RuntimePrivilegeContext::NormalUser,
        );
        let hooks = MaintenanceHookRunner::new(&inner, &session);

        assert!(hooks
            .capture_with_privilege(
                &NativeCommand::new("/bin/true"),
                PrivilegeRequirement::RootRequired
            )
            .is_err());
        assert!(hooks.execute(&root_plan).is_err());
        assert!(hooks
            .capture_with_privilege(
                &NativeCommand::new("/bin/true"),
                PrivilegeRequirement::Conditional
            )
            .is_err());
        assert_eq!(inner.privileged_captures.load(Ordering::Relaxed), 0);
        assert_eq!(inner.executes.load(Ordering::Relaxed), 0);

        hooks
            .capture_with_privilege(
                &NativeCommand::new("/bin/true"),
                PrivilegeRequirement::NoElevation,
            )
            .expect("non-root hook capture remains available");
        hooks
            .execute(&hook_plan(PrivilegeRequirement::NoElevation))
            .expect("non-root hook execution remains available");
        assert_eq!(inner.privileged_captures.load(Ordering::Relaxed), 1);
        assert_eq!(inner.executes.load(Ordering::Relaxed), 1);
    }

    impl ExecutionObserver for RecordingObserver {
        fn observe(&mut self, _plan: &ExecutionPlan, event: ProcessEvent) {
            self.events.push(event);
        }
    }

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

    #[cfg(unix)]
    #[test]
    fn execution_observer_receives_both_streams_without_changing_final_status() {
        let runner = StdProcessRunner;
        let plan = ExecutionPlan {
            backend_id: "test".to_owned(),
            backend_name: "Test".to_owned(),
            operation: crate::domain::OperationKind::Update,
            action: "Run test command".to_owned(),
            package_id: None,
            source: None,
            scope: None,
            details: Vec::new(),
            command: NativeCommand::new("/bin/sh").args(["-c", "printf stdout; printf stderr >&2"]),
            privilege: PrivilegeRequirement::NoElevation,
            requires_root: false,
            interactive: false,
        };
        let mut observer = RecordingObserver::default();

        let status = runner
            .execute_with_observer(&plan, &mut observer)
            .expect("command should succeed");

        assert!(status.success);
        assert_eq!(status.stdout, "stdout");
        assert_eq!(status.stderr, "stderr");
        assert!(observer.events.iter().any(|event| {
            matches!(
                event,
                ProcessEvent::Output {
                    stream: ProcessOutputStream::Stdout,
                    bytes
                } if bytes == b"stdout"
            )
        }));
        assert!(observer.events.iter().any(|event| {
            matches!(
                event,
                ProcessEvent::Output {
                    stream: ProcessOutputStream::Stderr,
                    bytes
                } if bytes == b"stderr"
            )
        }));
    }
}
