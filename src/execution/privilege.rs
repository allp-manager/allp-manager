use crate::domain::{
    AllpError, AllpResult, ExecutionPlan, NativeCommand, OriginalUser, PrivilegeRequirement,
    PrivilegeStatus, RuntimePrivilegeContext,
};
use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Output, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

#[cfg(unix)]
use std::os::unix::{
    fs::{MetadataExt, PermissionsExt},
    io::AsRawFd,
};

const AUTHENTICATION_TIMEOUT: Duration = Duration::from_secs(60);
const AUTHENTICATION_POLL_INTERVAL: Duration = Duration::from_millis(25);
const AUTHENTICATION_STOP_GRACE: Duration = Duration::from_secs(1);
const MAX_PRIVILEGE_DIAGNOSTIC_BYTES: usize = 32 * 1024;

fn authentication_timeout() -> Duration {
    #[cfg(debug_assertions)]
    if let Some(milliseconds) = env::var("ALLP_TEST_SUDO_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
    {
        return Duration::from_millis(milliseconds.max(1));
    }

    AUTHENTICATION_TIMEOUT
}

/// How the current privilege session was established.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivilegeAuthMethod {
    NotAttempted,
    NotRequired,
    AlreadyRoot,
    InteractiveValidation,
    NonInteractiveValidation,
}

/// Reusable administrator-authentication state for one Allp execution run.
///
/// Allp validates sudo once before a live maintenance UI starts.  Every
/// privileged child then runs with `sudo -n`, so an expired credential can be
/// represented as a structured state rather than opening an interactive
/// password prompt while the UI owns terminal rendering.
#[derive(Debug, Clone)]
pub struct PrivilegeSession {
    required: bool,
    authenticated: bool,
    authentication_method: PrivilegeAuthMethod,
    validated_at: Option<Instant>,
    context: RuntimePrivilegeContext,
    status: Option<PrivilegeStatus>,
    sudo: Option<PathBuf>,
}

impl PrivilegeSession {
    pub fn for_plans(plans: &[ExecutionPlan], context: &RuntimePrivilegeContext) -> Self {
        let required = plans
            .iter()
            .any(|plan| plan.privilege == PrivilegeRequirement::RootRequired);
        let (authenticated, authentication_method, status) = if !required {
            (
                false,
                PrivilegeAuthMethod::NotRequired,
                Some(PrivilegeStatus::NotRequired),
            )
        } else if context.is_root() {
            (
                true,
                PrivilegeAuthMethod::AlreadyRoot,
                Some(PrivilegeStatus::AlreadyRoot),
            )
        } else {
            (false, PrivilegeAuthMethod::NotAttempted, None)
        };

        Self {
            required,
            authenticated,
            authentication_method,
            validated_at: None,
            context: context.clone(),
            status,
            sudo: None,
        }
    }

    pub fn context(&self) -> &RuntimePrivilegeContext {
        &self.context
    }

    pub fn required(&self) -> bool {
        self.required
    }

    pub fn authenticated(&self) -> bool {
        self.authenticated
    }

    pub fn authentication_method(&self) -> PrivilegeAuthMethod {
        self.authentication_method
    }

    pub fn validated_at(&self) -> Option<Instant> {
        self.validated_at
    }

    pub fn status(&self) -> Option<PrivilegeStatus> {
        self.status
    }

    /// Records that a dynamically planned operation will require sudo.
    ///
    /// Maintenance follow-up plans can be discovered after the initial queue
    /// was authenticated. They join the same session and are authenticated at
    /// their own execution boundary rather than falling back to legacy sudo.
    pub fn ensure_for_plan(&mut self, plan: &ExecutionPlan) -> bool {
        if plan.privilege != PrivilegeRequirement::RootRequired {
            return false;
        }
        if !self.required {
            self.required = true;
            self.status = None;
            self.authenticated = false;
            self.authentication_method = PrivilegeAuthMethod::NotAttempted;
            self.validated_at = None;
        }
        true
    }

    /// Performs the one authentication preflight that may be interactive.
    ///
    /// It deliberately launches sudo directly with all three standard streams
    /// inherited.  In particular, callers must invoke this before a live TUI
    /// starts observing child output.
    pub fn preflight(&mut self, interactive: bool) -> PrivilegeStatus {
        if !self.required {
            self.authentication_method = PrivilegeAuthMethod::NotRequired;
            return self.set_status(PrivilegeStatus::NotRequired, false);
        }
        if self.context.is_root() {
            self.authentication_method = PrivilegeAuthMethod::AlreadyRoot;
            return self.set_status(PrivilegeStatus::AlreadyRoot, true);
        }

        let sudo = match resolve_sudo() {
            Ok(sudo) => sudo,
            Err(_) => return self.set_status(PrivilegeStatus::Unavailable, false),
        };

        let authentication_method = if interactive {
            PrivilegeAuthMethod::InteractiveValidation
        } else {
            PrivilegeAuthMethod::NonInteractiveValidation
        };
        let result = if interactive {
            run_interactive_validation(&sudo)
                .map(|status| classify_interactive_validation_failure(&status))
        } else {
            run_noninteractive_validation(&sudo)
                .map(|output| classify_noninteractive_validation_output(&output))
        };

        match result {
            Ok(PrivilegeStatus::Authenticated) => {
                self.sudo = Some(sudo);
                self.authentication_method = authentication_method;
                self.set_status(PrivilegeStatus::Authenticated, true)
            }
            Ok(status) => {
                self.authentication_method = authentication_method;
                self.set_status(status, false)
            }
            Err(status) => {
                self.authentication_method = authentication_method;
                self.set_status(status, false)
            }
        }
    }

    /// Verifies that a preflighted sudo credential is still usable without
    /// ever allowing a password prompt.  This runs before a root-required
    /// operation is rendered as running by the live UI.
    pub fn validate_for(&mut self, plan: &ExecutionPlan) -> PrivilegeStatus {
        if !plan.privilege.requires_sudo(&self.context) {
            return self.current_status_for(plan);
        }

        self.ensure_for_plan(plan);

        if !self.authenticated {
            // A dynamically added root plan has not had an initial preflight
            // yet. Treat it like expired credentials so the operation layer
            // can safely leave the TUI and authenticate once.
            return self.status.unwrap_or(PrivilegeStatus::CredentialExpired);
        }
        let Some(sudo) = self.sudo.as_deref() else {
            return self.set_status(PrivilegeStatus::Unavailable, false);
        };

        match run_noninteractive_validation(sudo) {
            Ok(output) => self.set_status(
                classify_noninteractive_validation_output(&output),
                output.status.success(),
            ),
            Err(status) => self.set_status(status, false),
        }
    }

    /// Returns the cached privilege outcome for a plan without spawning a
    /// process.  The central runner uses this as a final guard after the
    /// operation layer has completed its noninteractive validation.
    pub fn current_status_for(&self, plan: &ExecutionPlan) -> PrivilegeStatus {
        if !plan.privilege.requires_sudo(&self.context) {
            return if plan.privilege == PrivilegeRequirement::RootRequired && self.context.is_root()
            {
                PrivilegeStatus::AlreadyRoot
            } else {
                PrivilegeStatus::NotRequired
            };
        }
        if self.authenticated {
            PrivilegeStatus::Authenticated
        } else {
            self.status.unwrap_or(PrivilegeStatus::Unavailable)
        }
    }

    pub fn mark_noninteractive_failure(&mut self, status: PrivilegeStatus) {
        debug_assert!(matches!(
            status,
            PrivilegeStatus::CredentialExpired | PrivilegeStatus::Unavailable
        ));
        self.set_status(status, false);
    }

    fn sudo_path(&self) -> Option<&Path> {
        self.sudo.as_deref()
    }

    fn set_status(&mut self, status: PrivilegeStatus, authenticated: bool) -> PrivilegeStatus {
        self.status = Some(status);
        self.authenticated = authenticated;
        self.validated_at = authenticated.then(Instant::now);
        status
    }
}

fn classify_interactive_validation_failure(status: &ExitStatus) -> PrivilegeStatus {
    if status.success() {
        return PrivilegeStatus::Authenticated;
    }
    #[cfg(unix)]
    if std::os::unix::process::ExitStatusExt::signal(status) == Some(libc::SIGINT) {
        return PrivilegeStatus::AuthenticationCancelled;
    }

    if status.code() == Some(130) {
        return PrivilegeStatus::AuthenticationCancelled;
    }
    PrivilegeStatus::AuthenticationFailed
}

/// Executes `sudo -n -v` with a real stdin/TTY but captures its small
/// diagnostics. `-n` guarantees sudo cannot read a password, while inheriting
/// stdin avoids a false failure on installations using `requiretty`.
fn run_noninteractive_validation(sudo: &Path) -> Result<Output, PrivilegeStatus> {
    let _signals = AuthenticationSignalGuard::install();
    let mut child = Command::new(sudo)
        .args(["-n", "-v"])
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("LC_MESSAGES", "C")
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::Interrupted => PrivilegeStatus::AuthenticationCancelled,
            std::io::ErrorKind::TimedOut => PrivilegeStatus::AuthenticationTimedOut,
            _ => PrivilegeStatus::Unavailable,
        })?;
    let (sender, receiver) = mpsc::channel();
    if let Some(stdout) = child.stdout.take() {
        drain_validation_stream(stdout, sender.clone(), true);
    }
    if let Some(stderr) = child.stderr.take() {
        drain_validation_stream(stderr, sender.clone(), false);
    }
    drop(sender);
    let started = Instant::now();
    let status = loop {
        if AuthenticationSignalGuard::interrupted() {
            stop_authentication_child(&mut child);
            return Err(PrivilegeStatus::AuthenticationCancelled);
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() >= authentication_timeout() => {
                stop_authentication_child(&mut child);
                return Err(PrivilegeStatus::AuthenticationTimedOut);
            }
            Ok(None) => thread::sleep(AUTHENTICATION_POLL_INTERVAL),
            Err(error) => {
                stop_authentication_child(&mut child);
                return Err(match error.kind() {
                    std::io::ErrorKind::Interrupted => PrivilegeStatus::AuthenticationCancelled,
                    _ => PrivilegeStatus::Unavailable,
                });
            }
        }
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    for _ in 0..2 {
        match receiver.recv_timeout(AUTHENTICATION_STOP_GRACE) {
            Ok((is_stdout, bytes)) if is_stdout => stdout = bytes,
            Ok((_, bytes)) => stderr = bytes,
            Err(_) => return Err(PrivilegeStatus::Unavailable),
        }
    }
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn drain_validation_stream<R>(mut stream: R, sender: mpsc::Sender<(bool, Vec<u8>)>, stdout: bool)
where
    R: std::io::Read + Send + 'static,
{
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            match stream.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    let remaining = MAX_PRIVILEGE_DIAGNOSTIC_BYTES.saturating_sub(bytes.len());
                    bytes.extend_from_slice(&buffer[..count.min(remaining)]);
                }
            }
        }
        let _ = sender.send((stdout, bytes));
    });
}

fn classify_noninteractive_validation_output(output: &Output) -> PrivilegeStatus {
    if output.status.success() {
        return PrivilegeStatus::Authenticated;
    }
    #[cfg(unix)]
    if std::os::unix::process::ExitStatusExt::signal(&output.status) == Some(libc::SIGINT) {
        return PrivilegeStatus::AuthenticationCancelled;
    }
    if output.status.code() == Some(130) {
        return PrivilegeStatus::AuthenticationCancelled;
    }
    let diagnostics = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    classify_noninteractive_sudo_diagnostics(&diagnostics)
}

/// Maps C-locale `sudo -n` diagnostics to an administrator boundary state.
///
/// A password-specific failure means a previously authenticated credential
/// may be renewed outside the TUI. Policy, TTY, and helper failures are not
/// retried interactively because doing so would only repeat an unavailable
/// configuration.
pub(crate) fn classify_noninteractive_sudo_diagnostics(diagnostics: &str) -> PrivilegeStatus {
    let diagnostics = diagnostics.to_ascii_lowercase();
    if diagnostics.contains("a password is required")
        || diagnostics.contains("password is required")
        || diagnostics.contains("password is needed")
        || diagnostics.contains("no password was provided")
    {
        PrivilegeStatus::CredentialExpired
    } else {
        PrivilegeStatus::Unavailable
    }
}

/// Runs the only interactive sudo command with a short, owned deadline.
///
/// The standard streams remain inherited so sudo owns the real terminal. The
/// terminal snapshot is restored on every return path as a defensive guard
/// for current and future TUI implementations.
fn run_interactive_validation(sudo: &Path) -> Result<ExitStatus, PrivilegeStatus> {
    let terminal_state = TerminalState::capture();
    let _signals = AuthenticationSignalGuard::install();
    let child = Command::new(sudo)
        .arg("-v")
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("LC_MESSAGES", "C")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(error) => {
            #[cfg(unix)]
            if let Some(terminal_state) = terminal_state {
                terminal_state.restore();
            }
            #[cfg(not(unix))]
            terminal_state.restore();
            return Err(match error.kind() {
                std::io::ErrorKind::Interrupted => PrivilegeStatus::AuthenticationCancelled,
                std::io::ErrorKind::TimedOut => PrivilegeStatus::AuthenticationTimedOut,
                _ => PrivilegeStatus::Unavailable,
            });
        }
    };
    let started = Instant::now();
    let result = loop {
        if AuthenticationSignalGuard::interrupted() {
            stop_authentication_child(&mut child);
            break Err(PrivilegeStatus::AuthenticationCancelled);
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if started.elapsed() >= authentication_timeout() => {
                stop_authentication_child(&mut child);
                break Err(PrivilegeStatus::AuthenticationTimedOut);
            }
            Ok(None) => thread::sleep(AUTHENTICATION_POLL_INTERVAL),
            Err(error) => {
                stop_authentication_child(&mut child);
                break Err(match error.kind() {
                    std::io::ErrorKind::Interrupted => PrivilegeStatus::AuthenticationCancelled,
                    _ => PrivilegeStatus::Unavailable,
                });
            }
        }
    };
    #[cfg(unix)]
    if let Some(terminal_state) = terminal_state {
        terminal_state.restore();
    }
    #[cfg(not(unix))]
    terminal_state.restore();
    result
}

fn stop_authentication_child(child: &mut Child) {
    #[cfg(unix)]
    unsafe {
        libc::kill(child.id() as libc::pid_t, libc::SIGTERM);
    }
    let deadline = Instant::now() + AUTHENTICATION_STOP_GRACE;
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => return,
            Ok(None) => thread::sleep(AUTHENTICATION_POLL_INTERVAL),
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
#[derive(Clone, Copy)]
struct TerminalState {
    fd: libc::c_int,
    attributes: libc::termios,
}

#[cfg(unix)]
impl TerminalState {
    fn capture() -> Option<Self> {
        let fd = std::io::stdin().as_raw_fd();
        if unsafe { libc::isatty(fd) } != 1 {
            return None;
        }
        let mut attributes = unsafe { std::mem::zeroed::<libc::termios>() };
        (unsafe { libc::tcgetattr(fd, &mut attributes) } == 0).then_some(Self { fd, attributes })
    }

    fn restore(self) {
        unsafe {
            libc::tcsetattr(self.fd, libc::TCSANOW, &self.attributes);
        }
    }
}

#[cfg(not(unix))]
struct TerminalState;

#[cfg(not(unix))]
impl TerminalState {
    fn capture() -> Self {
        Self
    }

    fn restore(self) {}
}

#[cfg(unix)]
static AUTHENTICATION_INTERRUPTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[cfg(unix)]
extern "C" fn authentication_signal_handler(_: libc::c_int) {
    AUTHENTICATION_INTERRUPTED.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Temporarily owns SIGINT/SIGTERM while the authentication child owns the
/// terminal. The parent remains alive long enough to restore its terminal
/// snapshot and return a structured cancellation result.
#[cfg(unix)]
struct AuthenticationSignalGuard {
    previous_int: libc::sigaction,
    previous_term: libc::sigaction,
}

#[cfg(unix)]
impl AuthenticationSignalGuard {
    fn install() -> Option<Self> {
        AUTHENTICATION_INTERRUPTED.store(false, std::sync::atomic::Ordering::Relaxed);
        let mut action = unsafe { std::mem::zeroed::<libc::sigaction>() };
        action.sa_sigaction = authentication_signal_handler as *const () as usize;
        unsafe {
            libc::sigemptyset(&mut action.sa_mask);
        }
        let mut previous_int = unsafe { std::mem::zeroed::<libc::sigaction>() };
        let mut previous_term = unsafe { std::mem::zeroed::<libc::sigaction>() };
        if unsafe { libc::sigaction(libc::SIGINT, &action, &mut previous_int) } != 0 {
            return None;
        }
        if unsafe { libc::sigaction(libc::SIGTERM, &action, &mut previous_term) } != 0 {
            unsafe {
                libc::sigaction(libc::SIGINT, &previous_int, std::ptr::null_mut());
            }
            return None;
        }
        Some(Self {
            previous_int,
            previous_term,
        })
    }

    fn interrupted() -> bool {
        AUTHENTICATION_INTERRUPTED.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[cfg(unix)]
impl Drop for AuthenticationSignalGuard {
    fn drop(&mut self) {
        unsafe {
            libc::sigaction(libc::SIGINT, &self.previous_int, std::ptr::null_mut());
            libc::sigaction(libc::SIGTERM, &self.previous_term, std::ptr::null_mut());
        }
    }
}

#[cfg(not(unix))]
struct AuthenticationSignalGuard;

#[cfg(not(unix))]
impl AuthenticationSignalGuard {
    fn install() -> Option<Self> {
        None
    }

    fn interrupted() -> bool {
        false
    }
}

pub fn is_effective_root() -> bool {
    runtime_context().is_root()
}

pub fn runtime_context() -> RuntimePrivilegeContext {
    let effective_uid = effective_uid();

    if effective_uid != Some(0) {
        return RuntimePrivilegeContext::NormalUser;
    }

    if let Some(account) = validated_sudo_account() {
        return RuntimePrivilegeContext::SudoRootWithOriginalUser(OriginalUser {
            name: account.name,
            uid: Some(account.uid),
            gid: Some(account.gid),
        });
    }

    RuntimePrivilegeContext::RootDirect
}

#[cfg(unix)]
fn effective_uid() -> Option<u32> {
    if let Ok(status) = fs::read_to_string("/proc/self/status") {
        if let Some(uid) = status.lines().find_map(|line| {
            let values = line.strip_prefix("Uid:")?;
            values.split_whitespace().nth(1)?.parse::<u32>().ok()
        }) {
            return Some(uid);
        }
    }
    Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|value| value.trim().parse().ok())
}

#[cfg(not(unix))]
fn effective_uid() -> Option<u32> {
    None
}

pub fn prepare_command(
    command: &NativeCommand,
    privilege: PrivilegeRequirement,
) -> AllpResult<Command> {
    prepare_command_with_context(command, privilege, &runtime_context())
}

pub fn prepare_command_with_context(
    command: &NativeCommand,
    privilege: PrivilegeRequirement,
    context: &RuntimePrivilegeContext,
) -> AllpResult<Command> {
    prepare_command_with_context_for_effective_uid(
        command,
        privilege,
        context,
        effective_uid(),
        None,
    )
}

/// Prepares a command for a privilege session that has already completed its
/// sudo preflight.  Root-required children are deliberately noninteractive:
/// `sudo -n -- <native-command>` can never steal terminal input from a live
/// TUI.
pub fn prepare_command_with_privilege_session(
    command: &NativeCommand,
    privilege: PrivilegeRequirement,
    session: &PrivilegeSession,
) -> AllpResult<Command> {
    if privilege.requires_sudo(session.context()) && !session.authenticated {
        return Err(AllpError::InvalidInput(
            "refusing to start a privileged child without an authenticated privilege session"
                .to_owned(),
        ));
    }
    prepare_command_with_context_for_effective_uid_with_sudo_mode(
        command,
        privilege,
        session.context(),
        effective_uid(),
        session.sudo_path(),
        RootSudoMode::NonInteractive,
    )
}

fn prepare_command_with_context_for_effective_uid(
    command: &NativeCommand,
    privilege: PrivilegeRequirement,
    context: &RuntimePrivilegeContext,
    current_uid: Option<u32>,
    sudo_override: Option<&Path>,
) -> AllpResult<Command> {
    prepare_command_with_context_for_effective_uid_with_sudo_mode(
        command,
        privilege,
        context,
        current_uid,
        sudo_override,
        RootSudoMode::Interactive,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RootSudoMode {
    Interactive,
    NonInteractive,
}

fn prepare_command_with_context_for_effective_uid_with_sudo_mode(
    command: &NativeCommand,
    privilege: PrivilegeRequirement,
    context: &RuntimePrivilegeContext,
    current_uid: Option<u32>,
    sudo_override: Option<&Path>,
    root_sudo_mode: RootSudoMode,
) -> AllpResult<Command> {
    let root_required_program = (privilege == PrivilegeRequirement::RootRequired)
        .then(|| resolve_root_required_executable(&command.program))
        .transpose()?;
    let program = root_required_program.as_deref().unwrap_or(&command.program);

    let mut process = if privilege.requires_sudo(context) {
        let sudo = sudo_override
            .map(Path::to_path_buf)
            .map_or_else(resolve_sudo, Ok)?;
        let mut process = Command::new(sudo);
        if root_sudo_mode == RootSudoMode::NonInteractive {
            process.arg("-n");
        }
        process.arg("--").arg(program);
        process
    } else if privilege.requires_original_user(context) {
        let Some(user) = context.original_user() else {
            return Err(AllpError::InvalidInput(
                "refusing to run a user-scoped operation as root without an original sudo user"
                    .to_owned(),
            ));
        };
        return prepare_original_user_command(command, user, current_uid, sudo_override);
    } else if privilege == PrivilegeRequirement::OriginalUserRequired
        && matches!(context, RuntimePrivilegeContext::RootDirect)
    {
        return Err(AllpError::InvalidInput(
            "refusing to run a user-scoped operation as root without an original sudo user"
                .to_owned(),
        ));
    } else {
        Command::new(program)
    };

    process.args(&command.args);
    if let Some(current_dir) = &command.current_dir {
        process.current_dir(current_dir);
    }
    for (key, value) in &command.env {
        process.env(key, value);
    }
    if root_sudo_mode == RootSudoMode::NonInteractive && privilege.requires_sudo(context) {
        // Diagnostics at the sudo boundary have a stable locale. Native
        // programs may still select their own locale after sudo execs them.
        process.env("LC_ALL", "C");
        process.env("LANG", "C");
    }

    Ok(process)
}

fn user_path(home: &str) -> String {
    format!(
        "{home}/.local/bin:{home}/bin:/home/linuxbrew/.linuxbrew/bin:/opt/homebrew/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserAccount {
    pub name: String,
    pub uid: u32,
    pub gid: u32,
    pub home: PathBuf,
    pub shell: PathBuf,
}

/// Prepares a native command for one exact, system-database-validated non-root account.
///
/// This is intentionally owner-specific: callers do not rely on ambient `SUDO_USER` after
/// selecting an executable owner. When Allp is elevated, the prepared process de-escalates
/// through `sudo -H -u` and reconstructs the target account's deterministic environment.
pub struct UserContextExecutor;

impl UserContextExecutor {
    pub fn prepare(command: &NativeCommand, requested: &UserAccount) -> AllpResult<Command> {
        Self::prepare_for_effective_uid(command, requested, effective_uid(), None)
    }

    fn prepare_for_effective_uid(
        command: &NativeCommand,
        requested: &UserAccount,
        current_uid: Option<u32>,
        sudo_override: Option<&Path>,
    ) -> AllpResult<Command> {
        let account = validate_user_account(requested)?;
        if account.uid == 0 {
            return Err(AllpError::InvalidInput(
                "refusing to execute a user-scoped command as root".to_owned(),
            ));
        }

        let deescalating = current_uid == Some(0);
        if !deescalating && current_uid != Some(account.uid) {
            return Err(AllpError::InvalidInput(format!(
                "refusing to execute as {} (uid {}) from unrelated uid {:?}",
                account.name, account.uid, current_uid
            )));
        }

        let environment = user_environment(&account);
        if deescalating {
            validate_elevated_executable(&command.program)?;
            let sudo = sudo_override
                .map(Path::to_path_buf)
                .map_or_else(resolve_sudo, Ok)?;
            return Ok(build_deescalated_user_command(
                command,
                &account,
                &sudo,
                &environment,
            ));
        }

        let mut process = Command::new(&command.program);
        process.env_clear();
        for (key, value) in &environment {
            process.env(key, value);
        }

        process.args(&command.args);
        if let Some(current_dir) = &command.current_dir {
            process.current_dir(current_dir);
        }
        for (key, value) in &command.env {
            process.env(key, value);
        }
        Ok(process)
    }
}

fn build_deescalated_user_command(
    command: &NativeCommand,
    account: &UserAccount,
    sudo: &Path,
    environment: &[(String, String)],
) -> Command {
    let mut process = Command::new(sudo);
    process
        .arg("-H")
        .arg("-u")
        .arg(&account.name)
        .arg("--")
        .arg("/usr/bin/env")
        .arg("-i");
    for (key, value) in environment {
        process.arg(format!("{key}={value}"));
    }
    for (key, value) in &command.env {
        let mut assignment = key.clone();
        assignment.push("=");
        assignment.push(value);
        process.arg(assignment);
    }
    process.arg(&command.program).args(&command.args);
    if let Some(current_dir) = &command.current_dir {
        process.current_dir(current_dir);
    }
    process
}

fn prepare_original_user_command(
    command: &NativeCommand,
    user: &OriginalUser,
    current_uid: Option<u32>,
    sudo_override: Option<&Path>,
) -> AllpResult<Command> {
    let account = validate_original_user_account(user)?;
    UserContextExecutor::prepare_for_effective_uid(command, &account, current_uid, sudo_override)
}

fn validate_original_user_account(user: &OriginalUser) -> AllpResult<UserAccount> {
    let account = user_account_by_name(&user.name).ok_or_else(|| {
        AllpError::InvalidInput(format!(
            "original sudo user {} is not present in the system account database",
            user.name
        ))
    })?;
    if user.uid != Some(account.uid) || user.gid != Some(account.gid) {
        return Err(AllpError::InvalidInput(format!(
            "original sudo user {} no longer matches validated uid/gid {}:{}",
            user.name, account.uid, account.gid
        )));
    }
    Ok(account)
}

pub fn user_account_by_name(name: &str) -> Option<UserAccount> {
    #[cfg(not(unix))]
    {
        let _ = name;
        None
    }
    #[cfg(unix)]
    {
        let account = system_accounts()
            .into_iter()
            .find(|account| account.name == name);
        #[cfg(target_os = "macos")]
        {
            account.or_else(|| macos_account_by_name(name))
        }
        #[cfg(not(target_os = "macos"))]
        {
            account
        }
    }
}

pub fn user_account_by_uid(uid: u32) -> Option<UserAccount> {
    #[cfg(not(unix))]
    {
        let _ = uid;
        None
    }
    #[cfg(unix)]
    {
        let account = system_accounts()
            .into_iter()
            .find(|account| account.uid == uid);
        #[cfg(target_os = "macos")]
        {
            account.or_else(|| macos_account_by_uid(uid))
        }
        #[cfg(not(target_os = "macos"))]
        {
            account
        }
    }
}

pub fn user_group_ids(account: &UserAccount) -> BTreeSet<u32> {
    #[cfg(unix)]
    {
        let mut groups = BTreeSet::from([account.gid]);
        if let Ok(contents) = fs::read_to_string("/etc/group") {
            groups.extend(parse_supplementary_groups(&contents, &account.name));
        }
        #[cfg(target_os = "macos")]
        groups.extend(macos_group_ids(&account.name));
        groups
    }
    #[cfg(not(unix))]
    {
        BTreeSet::from([account.gid])
    }
}

fn validate_user_account(requested: &UserAccount) -> AllpResult<UserAccount> {
    let account = user_account_by_name(&requested.name).ok_or_else(|| {
        AllpError::InvalidInput(format!(
            "user {} is not present in the system account database",
            requested.name
        ))
    })?;
    if account.uid != requested.uid || account.gid != requested.gid {
        return Err(AllpError::InvalidInput(format!(
            "user {} no longer matches validated uid/gid {}:{}",
            requested.name, requested.uid, requested.gid
        )));
    }
    Ok(account)
}

fn user_environment(account: &UserAccount) -> Vec<(String, String)> {
    let home = account.home.to_string_lossy();
    let shell = account.shell.to_string_lossy();
    let mut environment = vec![
        ("LC_ALL".to_owned(), "C".to_owned()),
        ("LANG".to_owned(), "C".to_owned()),
        ("HOME".to_owned(), home.to_string()),
        ("USER".to_owned(), account.name.clone()),
        ("LOGNAME".to_owned(), account.name.clone()),
        ("PATH".to_owned(), user_path(&home)),
        ("SHELL".to_owned(), shell.to_string()),
        ("XDG_CONFIG_HOME".to_owned(), format!("{home}/.config")),
        ("XDG_CACHE_HOME".to_owned(), format!("{home}/.cache")),
        ("XDG_DATA_HOME".to_owned(), format!("{home}/.local/share")),
        ("XDG_STATE_HOME".to_owned(), format!("{home}/.local/state")),
    ];
    let runtime = format!("/run/user/{}", account.uid);
    if Path::new(&runtime).is_dir() {
        environment.push(("XDG_RUNTIME_DIR".to_owned(), runtime));
    }
    environment
}

fn validated_sudo_account() -> Option<UserAccount> {
    let name = env::var("SUDO_USER").ok()?;
    if name.is_empty() || name == "root" {
        return None;
    }
    let account = user_account_by_name(&name)?;
    if !env_identity_field_matches("SUDO_UID", account.uid)
        || !env_identity_field_matches("SUDO_GID", account.gid)
    {
        return None;
    }
    Some(account)
}

fn env_identity_field_matches(key: &str, expected: u32) -> bool {
    match env::var(key) {
        Ok(value) => value.parse::<u32>() == Ok(expected),
        Err(env::VarError::NotPresent | env::VarError::NotUnicode(_)) => false,
    }
}

#[cfg(unix)]
fn system_accounts() -> Vec<UserAccount> {
    fs::read_to_string("/etc/passwd")
        .ok()
        .map(|contents| parse_passwd(&contents))
        .unwrap_or_default()
}

#[cfg(unix)]
fn parse_passwd(contents: &str) -> Vec<UserAccount> {
    contents
        .lines()
        .filter_map(|line| {
            let fields = line.split(':').collect::<Vec<_>>();
            if fields.len() < 7 || fields[0].is_empty() || fields[5].is_empty() {
                return None;
            }
            Some(UserAccount {
                name: fields[0].to_owned(),
                uid: fields[2].parse().ok()?,
                gid: fields[3].parse().ok()?,
                home: PathBuf::from(fields[5]),
                shell: PathBuf::from(if fields[6].is_empty() {
                    "/bin/sh"
                } else {
                    fields[6]
                }),
            })
        })
        .collect()
}

/// macOS stores normal GUI accounts in Directory Services, where they need not appear in
/// `/etc/passwd`. Homebrew must validate the exact sudo user before it de-escalates, so use the
/// system Directory Services command as a narrowly scoped fallback rather than trusting the
/// ambient `SUDO_USER` fields by themselves.
#[cfg(target_os = "macos")]
fn macos_account_by_name(name: &str) -> Option<UserAccount> {
    let record = macos_user_record_path(name)?;
    let output = Command::new("/usr/bin/dscl")
        .args([
            ".",
            "-read",
            &record,
            "RecordName",
            "UniqueID",
            "PrimaryGroupID",
            "NFSHomeDirectory",
            "UserShell",
        ])
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let output = String::from_utf8(output.stdout).ok()?;
    parse_macos_dscl_account(name, &output)
}

#[cfg(target_os = "macos")]
fn macos_account_by_uid(uid: u32) -> Option<UserAccount> {
    let search_value = uid.to_string();
    let output = Command::new("/usr/bin/dscl")
        .args([".", "-search", "/Users", "UniqueID", &search_value])
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let output = String::from_utf8(output.stdout).ok()?;
    let names = output
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| macos_user_record_path(name).is_some())
        .collect::<BTreeSet<_>>();
    if names.len() != 1 {
        return None;
    }
    let name = names.into_iter().next()?;
    let account = macos_account_by_name(name)?;
    (account.uid == uid).then_some(account)
}

#[cfg(target_os = "macos")]
fn macos_group_ids(name: &str) -> BTreeSet<u32> {
    Command::new("/usr/bin/id")
        .args(["-G", name])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .filter_map(|group| group.parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(target_os = "macos")]
fn macos_user_record_path(name: &str) -> Option<String> {
    (!name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.chars().any(char::is_control))
    .then(|| format!("/Users/{name}"))
}

#[cfg(any(target_os = "macos", test))]
fn parse_macos_dscl_account(requested_name: &str, output: &str) -> Option<UserAccount> {
    let mut record_matches = false;
    let mut uid = None;
    let mut gid = None;
    let mut home = None;
    let mut shell = None;

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let Some((attribute, value)) = line.split_once(':') else {
            continue;
        };
        let attribute = attribute.trim();
        let value = value.trim();
        match attribute {
            "RecordName" => {
                record_matches = value.split_whitespace().any(|name| name == requested_name);
            }
            "UniqueID" => uid = macos_dscl_number(value),
            "PrimaryGroupID" => gid = macos_dscl_number(value),
            "NFSHomeDirectory" => home = macos_dscl_path(value),
            "UserShell" => shell = macos_dscl_path(value),
            _ => {}
        }
    }

    record_matches.then_some(UserAccount {
        name: requested_name.to_owned(),
        uid: uid?,
        gid: gid?,
        home: home?,
        shell: shell.unwrap_or_else(|| PathBuf::from("/bin/sh")),
    })
}

#[cfg(any(target_os = "macos", test))]
fn macos_dscl_number(value: &str) -> Option<u32> {
    let mut values = value.split_whitespace();
    let value = values.next()?;
    values.next().is_none().then(|| value.parse().ok())?
}

#[cfg(any(target_os = "macos", test))]
fn macos_dscl_path(value: &str) -> Option<PathBuf> {
    let path = PathBuf::from(value.trim());
    path.is_absolute().then_some(path)
}

#[cfg(unix)]
fn parse_supplementary_groups(contents: &str, user: &str) -> BTreeSet<u32> {
    contents
        .lines()
        .filter_map(|line| {
            let fields = line.split(':').collect::<Vec<_>>();
            if fields.len() < 4 || !fields[3].split(',').any(|member| member == user) {
                return None;
            }
            fields[2].parse().ok()
        })
        .collect()
}

fn resolve_sudo() -> AllpResult<PathBuf> {
    #[cfg(debug_assertions)]
    if let Some(path) = env::var_os("ALLP_TEST_SUDO_EXECUTABLE").map(PathBuf::from) {
        validate_elevated_executable(&path)?;
        return Ok(path);
    }
    resolve_trusted_root_helper("sudo")
}

fn resolve_root_required_executable(path: &Path) -> AllpResult<PathBuf> {
    // Integration tests exercise the full sudo command construction with isolated fake
    // executables. This escape hatch is absent from release builds and is coupled to the
    // already-explicit fake-sudo test boundary.
    #[cfg(debug_assertions)]
    if env::var_os("ALLP_TEST_SUDO_EXECUTABLE").is_some() {
        validate_elevated_executable(path)?;
        return fs::canonicalize(path).map_err(Into::into);
    }

    validate_trusted_root_executable(path, "root-required executable")
}

fn resolve_trusted_root_helper(name: &str) -> AllpResult<PathBuf> {
    let candidates = trusted_root_helper_candidates(name, env::var_os("PATH").as_deref());

    let mut seen = BTreeSet::new();
    let mut rejected = Vec::new();
    for candidate in candidates {
        let Ok(resolved) = fs::canonicalize(&candidate) else {
            continue;
        };
        if !seen.insert(resolved.clone()) {
            continue;
        }
        match validate_trusted_root_helper(&resolved) {
            Ok(()) => return Ok(resolved),
            Err(error) => rejected.push(format!("{}: {error}", candidate.display())),
        }
    }
    let detail = if rejected.is_empty() {
        "no fixed or PATH candidate exists".to_owned()
    } else {
        format!("rejected candidate(s): {}", rejected.join("; "))
    };
    Err(AllpError::BackendNotDetected(format!(
        "trusted root-owned {name} helper was not found; {detail}"
    )))
}

fn trusted_root_helper_candidates(name: &str, path: Option<&std::ffi::OsStr>) -> Vec<PathBuf> {
    let mut candidates = vec![
        PathBuf::from("/usr/bin").join(name),
        PathBuf::from("/bin").join(name),
    ];
    candidates.extend(
        path.into_iter()
            .flat_map(env::split_paths)
            .filter(|directory| directory.is_absolute())
            .map(|directory| directory.join(name)),
    );
    candidates
}

fn validate_trusted_root_helper(path: &Path) -> AllpResult<()> {
    validate_trusted_root_executable(path, "trusted helper").map(|_| ())
}

fn validate_trusted_root_executable(path: &Path, subject: &str) -> AllpResult<PathBuf> {
    if !path.is_absolute() {
        return Err(AllpError::InvalidInput(format!(
            "{subject} path is not absolute: {}",
            path.display()
        )));
    }
    let resolved = fs::canonicalize(path)?;
    let metadata = fs::metadata(&resolved)?;
    if !metadata.is_file() {
        return Err(AllpError::InvalidInput(format!(
            "{subject} is not a regular file: {}",
            resolved.display()
        )));
    }

    #[cfg(unix)]
    {
        if metadata.uid() != 0 {
            return Err(AllpError::InvalidInput(format!(
                "{subject} is not owned by root: {}",
                resolved.display()
            )));
        }
        if metadata.permissions().mode() & 0o022 != 0 {
            return Err(AllpError::InvalidInput(format!(
                "{subject} is group/world-writable: {}",
                resolved.display()
            )));
        }
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(AllpError::InvalidInput(format!(
                "{subject} is not executable: {}",
                resolved.display()
            )));
        }
        let mut ancestor = resolved.parent();
        while let Some(directory) = ancestor {
            let metadata = fs::metadata(directory)?;
            if !metadata.is_dir()
                || metadata.uid() != 0
                || metadata.permissions().mode() & 0o022 != 0
            {
                return Err(AllpError::InvalidInput(format!(
                    "{subject} ancestor is not a root-owned, non-writable directory: {}",
                    directory.display()
                )));
            }
            ancestor = directory.parent();
        }
    }

    Ok(resolved)
}

fn validate_elevated_executable(path: &Path) -> AllpResult<()> {
    if !path.is_absolute() {
        return Err(AllpError::InvalidInput(format!(
            "refusing to elevate non-absolute executable path: {}",
            path.display()
        )));
    }

    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(AllpError::InvalidInput(format!(
            "refusing to elevate non-file executable path: {}",
            path.display()
        )));
    }

    #[cfg(unix)]
    {
        let mode = metadata.permissions().mode();
        if mode & 0o022 != 0 {
            return Err(AllpError::InvalidInput(format!(
                "refusing to elevate group/world-writable executable path: {}",
                path.display()
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::domain::{
        ExecutionPlan, NativeCommand, OperationKind, PrivilegeRequirement, RuntimePrivilegeContext,
    };

    fn root_plan() -> ExecutionPlan {
        ExecutionPlan {
            backend_id: "test".to_owned(),
            backend_name: "Test".to_owned(),
            operation: OperationKind::Update,
            action: "Test administrator boundary".to_owned(),
            package_id: None,
            source: None,
            scope: None,
            details: Vec::new(),
            command: NativeCommand::new("/bin/true"),
            privilege: PrivilegeRequirement::RootRequired,
            requires_root: true,
            interactive: false,
        }
    }

    #[test]
    fn noninteractive_sudo_diagnostics_preserve_expiry_vs_unavailable() {
        assert_eq!(
            super::classify_noninteractive_sudo_diagnostics("SUDO: A PASSWORD IS REQUIRED"),
            crate::domain::PrivilegeStatus::CredentialExpired
        );
        assert_eq!(
            super::classify_noninteractive_sudo_diagnostics("sudo: no password was provided"),
            crate::domain::PrivilegeStatus::CredentialExpired
        );
        for diagnostics in [
            "sudo: user is not in the sudoers file",
            "sudo: sorry, you must have a tty to run sudo",
            "sudo: policy denied this command",
            "",
        ] {
            assert_eq!(
                super::classify_noninteractive_sudo_diagnostics(diagnostics),
                crate::domain::PrivilegeStatus::Unavailable,
                "diagnostics: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn privilege_session_distinguishes_no_requirement_root_and_dynamic_root_plan() {
        let no_plan = super::PrivilegeSession::for_plans(&[], &RuntimePrivilegeContext::NormalUser);
        assert_eq!(
            no_plan.status(),
            Some(crate::domain::PrivilegeStatus::NotRequired)
        );
        assert!(!no_plan.required());

        let plan = root_plan();
        let root_session = super::PrivilegeSession::for_plans(
            std::slice::from_ref(&plan),
            &RuntimePrivilegeContext::RootDirect,
        );
        assert_eq!(
            root_session.status(),
            Some(crate::domain::PrivilegeStatus::AlreadyRoot)
        );
        assert!(root_session.authenticated());

        let mut dynamic =
            super::PrivilegeSession::for_plans(&[], &RuntimePrivilegeContext::NormalUser);
        assert!(dynamic.ensure_for_plan(&plan));
        assert!(dynamic.required());
        assert!(!dynamic.authenticated());
        assert_eq!(
            dynamic.authentication_method(),
            super::PrivilegeAuthMethod::NotAttempted
        );
    }

    #[cfg(unix)]
    #[test]
    fn noninteractive_validation_uses_allps_bounded_deadline() {
        static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

        let _lock = ENV_LOCK.lock().expect("test environment lock");
        let path = std::env::temp_dir().join(format!(
            "allp-sudo-validation-timeout-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("unnamed")
        ));
        std::fs::write(&path, "#!/bin/sh\nwhile :; do :; done\n")
            .expect("fake sudo should be written");
        let mut permissions = std::fs::metadata(&path)
            .expect("fake sudo metadata")
            .permissions();
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(0o700);
        std::fs::set_permissions(&path, permissions).expect("fake sudo should be executable");

        std::env::set_var("ALLP_TEST_SUDO_TIMEOUT_MS", "50");
        let started = std::time::Instant::now();
        let result = super::run_noninteractive_validation(&path);
        std::env::remove_var("ALLP_TEST_SUDO_TIMEOUT_MS");
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            result,
            Err(crate::domain::PrivilegeStatus::AuthenticationTimedOut)
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "validation must use Allp's deadline rather than sudo's multi-minute prompt timeout"
        );
    }

    #[cfg(unix)]
    fn non_root_account() -> super::UserAccount {
        super::system_accounts()
            .into_iter()
            .find(|account| account.uid != 0)
            .expect("at least one non-root system account is required")
    }

    #[cfg(unix)]
    #[test]
    fn passwd_parser_preserves_validated_user_environment() {
        let accounts = super::parse_passwd(
            "root:x:0:0:root:/root:/bin/bash\ntestuser:x:1000:1001:Test:/home/testuser:/bin/zsh\n",
        );
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[1].name, "testuser");
        assert_eq!(accounts[1].uid, 1000);
        assert_eq!(accounts[1].gid, 1001);
        assert_eq!(accounts[1].home, std::path::Path::new("/home/testuser"));
        assert_eq!(accounts[1].shell, std::path::Path::new("/bin/zsh"));
    }

    #[test]
    fn macos_directory_service_parser_validates_gui_account() {
        let account = super::parse_macos_dscl_account(
            "wrench",
            "RecordName: wrench\nUniqueID: 501\nPrimaryGroupID: 20\nNFSHomeDirectory: /Users/wrench\nUserShell: /bin/zsh\n",
        )
        .expect("a complete Directory Services account should parse");

        assert_eq!(account.name, "wrench");
        assert_eq!(account.uid, 501);
        assert_eq!(account.gid, 20);
        assert_eq!(account.home, std::path::Path::new("/Users/wrench"));
        assert_eq!(account.shell, std::path::Path::new("/bin/zsh"));
    }

    #[test]
    fn macos_directory_service_parser_rejects_mismatched_or_unsafe_account_data() {
        let mismatched = "RecordName: somebody-else\nUniqueID: 501\nPrimaryGroupID: 20\nNFSHomeDirectory: /Users/wrench\nUserShell: /bin/zsh\n";
        assert!(super::parse_macos_dscl_account("wrench", mismatched).is_none());

        let relative_home = "RecordName: wrench\nUniqueID: 501\nPrimaryGroupID: 20\nNFSHomeDirectory: Users/wrench\nUserShell: /bin/zsh\n";
        assert!(super::parse_macos_dscl_account("wrench", relative_home).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn supplementary_group_parser_matches_exact_members() {
        let groups = super::parse_supplementary_groups(
            "wheel:x:10:alice,bob\nusers:x:100:malice\ndev:x:200:alice\n",
            "alice",
        );
        assert_eq!(groups, std::collections::BTreeSet::from([10, 200]));
    }

    #[cfg(unix)]
    #[test]
    fn user_context_executor_rejects_root_target() {
        let root = super::user_account_by_uid(0).expect("root account should exist");
        let error = super::UserContextExecutor::prepare(
            &crate::domain::NativeCommand::new("/bin/true"),
            &root,
        )
        .expect_err("root target must be rejected");
        assert!(error.to_string().contains("as root"));
    }

    #[cfg(unix)]
    #[test]
    fn user_context_executor_revalidates_account_and_sets_user_environment() {
        let uid = super::effective_uid().expect("effective uid");
        if uid == 0 {
            return;
        }
        let account = super::user_account_by_uid(uid).expect("current account");
        let command = super::UserContextExecutor::prepare(
            &crate::domain::NativeCommand::new("/bin/true").env("HOMEBREW_NO_AUTO_UPDATE", "1"),
            &account,
        )
        .expect("current account context");
        assert_eq!(command.get_program(), "/bin/true");
        let environment = command
            .get_envs()
            .filter_map(|(key, value)| Some((key.to_str()?, value?.to_str()?)))
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(environment.get("HOME").copied(), account.home.to_str());
        assert_eq!(environment.get("LC_ALL").copied(), Some("C"));
        assert_eq!(environment.get("LANG").copied(), Some("C"));
        assert!(!environment.contains_key("BASH_ENV"));
        assert!(!environment.contains_key("RUBYOPT"));
        assert!(!environment.contains_key("HOMEBREW_PREFIX"));
        assert_eq!(
            environment.get("HOMEBREW_NO_AUTO_UPDATE").copied(),
            Some("1")
        );
        assert_eq!(
            environment.get("USER").copied(),
            Some(account.name.as_str())
        );

        let mut stale = account;
        stale.gid = stale.gid.saturating_add(1);
        let error = super::UserContextExecutor::prepare(
            &crate::domain::NativeCommand::new("/bin/true"),
            &stale,
        )
        .expect_err("changed account identity must be rejected");
        assert!(error.to_string().contains("no longer matches"));
    }

    #[cfg(unix)]
    #[test]
    fn elevated_generic_original_user_command_uses_sanitized_validated_context() {
        let account = non_root_account();
        let original = crate::domain::OriginalUser {
            name: account.name.clone(),
            uid: Some(account.uid),
            gid: Some(account.gid),
        };
        let context = crate::domain::RuntimePrivilegeContext::SudoRootWithOriginalUser(original);
        let native =
            crate::domain::NativeCommand::new("/bin/true").env("HOMEBREW_NO_AUTO_UPDATE", "1");

        let command = super::prepare_command_with_context_for_effective_uid(
            &native,
            crate::domain::PrivilegeRequirement::OriginalUserRequired,
            &context,
            Some(0),
            Some(std::path::Path::new("/usr/bin/sudo")),
        )
        .expect("validated original-user command");

        assert_eq!(command.get_program(), "/usr/bin/sudo");
        let arguments = command
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            &arguments[..6],
            [
                "-H",
                "-u",
                account.name.as_str(),
                "--",
                "/usr/bin/env",
                "-i"
            ]
        );
        assert!(arguments.iter().any(|value| value == "LC_ALL=C"));
        assert!(arguments.iter().any(|value| value == "LANG=C"));
        assert!(arguments
            .iter()
            .any(|value| value == "HOMEBREW_NO_AUTO_UPDATE=1"));
        assert!(!arguments.iter().any(|value| value.starts_with("BASH_ENV=")));
        assert!(!arguments.iter().any(|value| value.starts_with("RUBYOPT=")));
        assert!(!arguments
            .iter()
            .any(|value| value.starts_with("HOMEBREW_PREFIX=")));
        assert_eq!(arguments.last().map(String::as_str), Some("/bin/true"));
    }

    #[cfg(unix)]
    #[test]
    fn elevated_generic_original_user_command_rejects_changed_account_tuple() {
        let account = non_root_account();
        let context = crate::domain::RuntimePrivilegeContext::SudoRootWithOriginalUser(
            crate::domain::OriginalUser {
                name: account.name,
                uid: Some(account.uid),
                gid: Some(account.gid.saturating_add(1)),
            },
        );
        let error = super::prepare_command_with_context_for_effective_uid(
            &crate::domain::NativeCommand::new("/bin/true"),
            crate::domain::PrivilegeRequirement::OriginalUserRequired,
            &context,
            Some(0),
            Some(std::path::Path::new("/usr/bin/sudo")),
        )
        .expect_err("changed original-user identity must be rejected");
        assert!(error
            .to_string()
            .contains("no longer matches validated uid/gid"));
    }

    #[cfg(unix)]
    #[test]
    fn trusted_sudo_candidates_prefer_fixed_paths_and_ignore_relative_path_entries() {
        let candidates = super::trusted_root_helper_candidates(
            "sudo",
            Some(std::ffi::OsStr::new(
                "relative:/tmp/path-shadow:/usr/local/bin",
            )),
        );
        assert_eq!(candidates[0], std::path::Path::new("/usr/bin/sudo"));
        assert_eq!(candidates[1], std::path::Path::new("/bin/sudo"));
        assert_eq!(candidates[2], std::path::Path::new("/tmp/path-shadow/sudo"));
        assert!(!candidates
            .iter()
            .any(|candidate| candidate == std::path::Path::new("relative/sudo")));
    }

    #[cfg(unix)]
    #[test]
    fn trusted_sudo_validator_rejects_user_controlled_helper_or_ancestor() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "allp-untrusted-sudo-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("unnamed")
        ));
        std::fs::create_dir_all(&root).expect("test directory");
        let helper = root.join("sudo");
        std::fs::write(&helper, "#!/bin/sh\nexit 0\n").expect("test helper");
        let mut permissions = std::fs::metadata(&helper)
            .expect("test helper metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&helper, permissions).expect("test helper permissions");

        let error = super::validate_trusted_root_helper(&helper)
            .expect_err("user-controlled sudo helper must be rejected");
        assert!(error.to_string().contains("trusted helper"));
        std::fs::remove_dir_all(root).expect("test directory cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn root_required_validator_rejects_user_controlled_executable_or_ancestor() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "allp-untrusted-root-program-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("unnamed")
        ));
        std::fs::create_dir_all(&root).expect("test directory");
        let executable = root.join("apt-get");
        std::fs::write(&executable, "#!/bin/sh\nexit 0\n").expect("test executable");
        let mut permissions = std::fs::metadata(&executable)
            .expect("test executable metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&executable, permissions).expect("test executable permissions");

        let error =
            super::validate_trusted_root_executable(&executable, "root-required executable")
                .expect_err("a user-controlled root-required executable must be rejected");
        assert!(error.to_string().contains("root-required executable"));
        assert!(
            error.to_string().contains("not owned by root")
                || error.to_string().contains("ancestor is not a root-owned")
        );
        std::fs::remove_dir_all(root).expect("test directory cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn root_required_preparation_executes_the_validated_canonical_path() {
        use std::os::unix::fs::MetadataExt;

        let candidates = ["/usr/bin/true", "/bin/true", "/usr/bin/env"];
        let Some(source) = candidates.iter().map(std::path::Path::new).find(|path| {
            std::fs::metadata(path)
                .map(|metadata| metadata.uid() == 0)
                .unwrap_or(false)
        }) else {
            // Some sandboxed test environments remap host root-owned files to an
            // unprivileged overflow UID. The rejection test above remains effective there.
            return;
        };
        let expected = std::fs::canonicalize(source).expect("system true executable");
        let native = crate::domain::NativeCommand::new(source);

        let command = super::prepare_command_with_context_for_effective_uid(
            &native,
            crate::domain::PrivilegeRequirement::RootRequired,
            &crate::domain::RuntimePrivilegeContext::RootDirect,
            Some(0),
            None,
        )
        .expect("root-owned executable should pass the root boundary");

        assert_eq!(command.get_program(), expected.as_os_str());
    }
}
