//! Process execution helpers ported from `py/envoy/_executor.py`.
//!
//! [`ProcessExecutor`] handles executable resolution, command preparation,
//! line-oriented stdout/stderr draining, and best-effort process termination.
//! The implementation intentionally preserves a few Python behaviors that are
//! not the most idiomatic Rust choices:
//!
//! - [`ProcessExecutor::stream_process_output`] drains stdout to EOF before it
//!   starts draining stderr. This is *not* true interleaving, but it matches
//!   the original Python implementation exactly.
//! - [`ProcessExecutor::terminate_process`] is platform-dependent. Unix builds
//!   attempt a graceful `TERM` followed by a forced kill after five seconds.
//!   Windows has no portable stdlib equivalent of Python's `terminate()`, so
//!   the Rust port falls back to an immediate force-kill there.

use std::env;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Write};
use std::panic::{self, AssertUnwindSafe};
use std::path::{Component, Path, PathBuf};
use std::process::Child;

#[cfg(target_os = "windows")]
use std::ffi::OsStr;
#[cfg(unix)]
use std::process::Command;
#[cfg(unix)]
use std::thread;
#[cfg(unix)]
use std::time::{Duration, Instant};

use crate::error::{EnvoyError, Result};
use crate::models::{ErrorCallback, OutputCallback};

/// Handles executable resolution, command preparation, output streaming, and
/// best-effort process termination.
///
/// This ports Python's `ProcessExecutor` class into a small Rust utility type.
/// The callback fields intentionally use boxed trait objects, following the
/// same pattern used by [`crate::models::WrapperConfig`].
pub struct ProcessExecutor {
    /// Whether child output should be mirrored to the parent's stdout/stderr.
    pub stream_output: bool,
    /// Optional callback invoked for each drained stdout line.
    pub on_output: Option<Box<OutputCallback>>,
    /// Optional callback invoked for each drained stderr line.
    pub on_error: Option<Box<ErrorCallback>>,
}

impl ProcessExecutor {
    /// Create a new process executor with callback slots set to `None`.
    pub fn new(stream_output: bool) -> Self {
        Self {
            stream_output,
            on_output: None,
            on_error: None,
        }
    }

    /// Return a new executor with an stdout-line callback attached.
    pub fn with_on_output(mut self, callback: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.on_output = Some(Box::new(callback));
        self
    }

    /// Return a new executor with an stderr-line callback attached.
    pub fn with_on_error(mut self, callback: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.on_error = Some(Box::new(callback));
        self
    }

    /// Resolve an executable path, searching a PATH-like string if needed.
    ///
    /// If `executable` is absolute or contains directory components, this
    /// verifies that the path exists and returns an absolute path.
    ///
    /// If `executable` is a bare command name, resolution mirrors Python's
    /// `shutil.which(..., path=search_path)`: the provided `search_path` is
    /// searched first, falling back to the process `PATH` environment
    /// variable. On Windows, `PATHEXT` expansion is applied for bare names.
    pub fn resolve_executable(executable: &Path, search_path: Option<&str>) -> Result<PathBuf> {
        let executable_text = executable.to_string_lossy().into_owned();

        if executable.is_absolute() || has_directory_component(executable) {
            if !executable.exists() {
                return Err(EnvoyError::EnvironmentBuild(format!(
                    "Executable not found: {executable_text}"
                )));
            }

            return make_absolute(executable).map_err(|error| {
                EnvoyError::EnvironmentBuild(format!(
                    "Failed to build absolute executable path for \
                     '{executable_text}': {error}"
                ))
            });
        }

        if let Some(found_path) = find_in_path(executable, search_path) {
            return Ok(found_path);
        }

        Err(EnvoyError::EnvironmentBuild(format!(
            "Executable '{executable_text}' not found in PATH"
        )))
    }

    /// Prepare the full command vector for execution.
    ///
    /// On Windows, `.bat` and `.cmd` targets are wrapped with `cmd /c` to
    /// match Python's `CreateProcess` workaround for batch files.
    pub fn prepare_command(
        &self,
        executable: &Path,
        args: &[String],
        search_path: Option<&str>,
    ) -> Result<Vec<String>> {
        let resolved_executable = Self::resolve_executable(executable, search_path)?;
        let resolved_string = resolved_executable.to_string_lossy().into_owned();
        let mut command = Vec::with_capacity(args.len() + 1);
        command.push(resolved_string);
        command.extend(args.iter().cloned());

        #[cfg(target_os = "windows")]
        if is_batch_script(&resolved_executable) {
            let mut wrapped_command = Vec::with_capacity(command.len() + 2);
            wrapped_command.push(String::from("cmd"));
            wrapped_command.push(String::from("/c"));
            wrapped_command.extend(command);
            command = wrapped_command;
        }

        Ok(command)
    }

    /// Drain child stdout, then stderr, collecting and optionally echoing
    /// line-oriented output.
    ///
    /// This intentionally preserves the Python porting target's sequential
    /// behavior: stdout is fully drained before stderr. That means this method
    /// does not interleave the two streams in true real-time.
    ///
    /// Callback invocation is wrapped with `catch_unwind` so a panicking
    /// callback is logged and ignored, which is the closest Rust analogue to
    /// Python's "log callback error and continue" behavior.
    pub fn stream_process_output(&self, child: &mut Child) -> Result<(String, String)> {
        let stdout_lines = if let Some(stdout) = child.stdout.take() {
            drain_stream(
                stdout,
                self.stream_output,
                StreamKind::Stdout,
                self.on_output.as_deref(),
            )?
        } else {
            Vec::new()
        };

        let stderr_lines = if let Some(stderr) = child.stderr.take() {
            drain_stream(
                stderr,
                self.stream_output,
                StreamKind::Stderr,
                self.on_error.as_deref(),
            )?
        } else {
            Vec::new()
        };

        Ok((stdout_lines.join("\n"), stderr_lines.join("\n")))
    }

    /// Terminate a running process using best-effort platform-specific logic.
    ///
    /// `None` is a no-op. Errors are logged to stderr and never propagated,
    /// matching the Python helper's "swallow and log" contract.
    pub fn terminate_process(child: Option<&mut Child>) {
        let Some(child) = child else {
            return;
        };

        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => {}
            Err(error) => {
                eprintln!("Error checking process status before terminate: {error}");
            }
        }

        #[cfg(target_os = "windows")]
        terminate_process_windows(child);

        #[cfg(unix)]
        terminate_process_unix(child);
    }
}

#[derive(Clone, Copy)]
enum StreamKind {
    Stdout,
    Stderr,
}

fn has_directory_component(path: &Path) -> bool {
    path.components().nth(1).is_some()
}

fn make_absolute(path: &Path) -> std::io::Result<PathBuf> {
    if path.is_absolute() {
        return Ok(normalize_path(path));
    }

    Ok(normalize_path(&env::current_dir()?.join(path)))
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }

    normalized
}

fn find_in_path(executable: &Path, search_path: Option<&str>) -> Option<PathBuf> {
    let search_root = search_path
        .map(OsString::from)
        .or_else(|| env::var_os("PATH"))?;
    let path_entries = env::split_paths(&search_root);

    #[cfg(target_os = "windows")]
    let candidate_names = windows_candidate_names(executable);

    #[cfg(not(target_os = "windows"))]
    let candidate_names = vec![executable.to_path_buf()];

    for directory in path_entries {
        for candidate_name in &candidate_names {
            let candidate_path = directory.join(candidate_name);
            if is_executable_candidate(&candidate_path) {
                return Some(candidate_path);
            }
        }
    }

    None
}

#[cfg(target_os = "windows")]
fn windows_candidate_names(executable: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![executable.to_path_buf()];

    if executable.extension().is_some() {
        return candidates;
    }

    for extension in windows_pathexts() {
        let mut candidate = executable.as_os_str().to_os_string();
        candidate.push(extension);
        candidates.push(PathBuf::from(candidate));
    }

    candidates
}

#[cfg(target_os = "windows")]
fn windows_pathexts() -> Vec<OsString> {
    let pathext = env::var_os("PATHEXT").unwrap_or_else(|| OsString::from(".COM;.EXE;.BAT;.CMD"));

    pathext
        .to_string_lossy()
        .split(';')
        .filter(|entry| !entry.is_empty())
        .map(OsString::from)
        .collect()
}

#[cfg(target_os = "windows")]
fn is_executable_candidate(path: &Path) -> bool {
    path.exists() && !path.is_dir()
}

#[cfg(unix)]
fn is_executable_candidate(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    if !path.is_file() {
        return false;
    }

    match path.metadata() {
        Ok(metadata) => metadata.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

#[cfg(target_os = "windows")]
fn is_batch_script(path: &Path) -> bool {
    match path.extension().and_then(OsStr::to_str) {
        Some(extension) => {
            extension.eq_ignore_ascii_case("bat") || extension.eq_ignore_ascii_case("cmd")
        }
        None => false,
    }
}

fn drain_stream<R: std::io::Read>(
    stream: R,
    stream_output: bool,
    stream_kind: StreamKind,
    callback: Option<&(dyn Fn(&str) + Send + Sync)>,
) -> Result<Vec<String>> {
    let mut reader = BufReader::new(stream);
    let mut buffer = Vec::new();
    let mut lines = Vec::new();

    loop {
        buffer.clear();

        let bytes_read = reader.read_until(b'\n', &mut buffer).map_err(|error| {
            EnvoyError::Execution(format!(
                "Failed to read child {}: {error}",
                stream_kind.label()
            ))
        })?;

        if bytes_read == 0 {
            break;
        }

        while matches!(buffer.last(), Some(b'\n' | b'\r')) {
            buffer.pop();
        }

        let line = String::from_utf8_lossy(&buffer).trim_end().to_string();
        lines.push(line.clone());

        if stream_output {
            write_stream_line(stream_kind, &line).map_err(|error| {
                EnvoyError::Execution(format!(
                    "Failed to forward child {}: {error}",
                    stream_kind.label()
                ))
            })?;
        }

        invoke_callback(stream_kind, callback, &line);
    }

    Ok(lines)
}

impl StreamKind {
    fn label(&self) -> &'static str {
        match self {
            StreamKind::Stdout => "stdout",
            StreamKind::Stderr => "stderr",
        }
    }
}

fn write_stream_line(stream_kind: StreamKind, line: &str) -> std::io::Result<()> {
    match stream_kind {
        StreamKind::Stdout => {
            println!("{line}");
            std::io::stdout().flush()
        }
        StreamKind::Stderr => {
            eprintln!("{line}");
            std::io::stderr().flush()
        }
    }
}

fn invoke_callback(
    stream_kind: StreamKind,
    callback: Option<&(dyn Fn(&str) + Send + Sync)>,
    line: &str,
) {
    let Some(callback) = callback else {
        return;
    };

    if let Err(payload) = panic::catch_unwind(AssertUnwindSafe(|| callback(line))) {
        eprintln!(
            "{} callback error: {}",
            stream_kind.callback_label(),
            panic_payload_message(payload.as_ref())
        );
    }
}

impl StreamKind {
    fn callback_label(&self) -> &'static str {
        match self {
            StreamKind::Stdout => "onOutput",
            StreamKind::Stderr => "onError",
        }
    }
}

fn panic_payload_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        String::from("panic without string payload")
    }
}

#[cfg(target_os = "windows")]
fn terminate_process_windows(child: &mut Child) {
    if let Err(error) = child.kill() {
        eprintln!("Error terminating process: {error}");
        return;
    }

    if let Err(error) = child.wait() {
        eprintln!("Error waiting for terminated process: {error}");
    }
}

#[cfg(unix)]
fn terminate_process_unix(child: &mut Child) {
    let pid = child.id().to_string();

    match Command::new("kill").args(["-TERM", &pid]).status() {
        Ok(status) if status.success() => {}
        Ok(status) => {
            eprintln!(
                "Error terminating process gracefully: kill -TERM exited \
                 with status {status}"
            );
        }
        Err(error) => {
            eprintln!("Error terminating process gracefully: {error}");
        }
    }

    let deadline = Instant::now() + Duration::from_secs(5);

    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(100));
            }
            Ok(None) => {
                eprintln!("Process did not terminate gracefully, forcing kill...");
                break;
            }
            Err(error) => {
                eprintln!("Error waiting for process termination: {error}");
                break;
            }
        }
    }

    if let Err(error) = child.kill() {
        eprintln!("Error terminating process: {error}");
        return;
    }

    if let Err(error) = child.wait() {
        eprintln!("Error waiting for terminated process: {error}");
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::ProcessExecutor;
    use crate::error::EnvoyError;

    #[test]
    fn resolve_executable_finds_known_system_executable() {
        #[cfg(target_os = "windows")]
        let executable = PathBuf::from("cmd.exe");

        #[cfg(not(target_os = "windows"))]
        let executable = PathBuf::from("sh");

        let resolved = ProcessExecutor::resolve_executable(executable.as_path(), None)
            .expect("system executable should resolve from PATH");

        assert!(resolved.exists());
    }

    #[test]
    fn resolve_executable_errors_for_nonexistent_bare_name() {
        let executable = PathBuf::from("envoy-nonexistent-executable-for-tests-9c2c3a03");

        let error = ProcessExecutor::resolve_executable(executable.as_path(), None)
            .expect_err("missing bare executable should fail");

        assert!(matches!(error, EnvoyError::EnvironmentBuild(_)));
        assert_eq!(
            error.to_string(),
            "failed to build environment: Executable \
             'envoy-nonexistent-executable-for-tests-9c2c3a03' not found in \
             PATH"
        );
    }

    #[test]
    fn resolve_executable_accepts_existing_absolute_path() {
        let current_executable = std::env::current_exe().expect("current executable path");

        let resolved = ProcessExecutor::resolve_executable(current_executable.as_path(), None)
            .expect("existing absolute path should resolve");

        assert_eq!(resolved, current_executable);
    }

    #[test]
    fn resolve_executable_errors_for_missing_absolute_path() {
        let missing_path = unique_test_path("envoy_missing_absolute_executable.exe");

        let error = ProcessExecutor::resolve_executable(missing_path.as_path(), None)
            .expect_err("missing absolute path should fail");

        assert!(matches!(error, EnvoyError::EnvironmentBuild(_)));
        assert_eq!(
            error.to_string(),
            format!(
                "failed to build environment: Executable not found: {}",
                missing_path.display()
            )
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn resolve_executable_uses_search_path_and_pathext() {
        let command_path = unique_test_path("envoy_executor_test.cmd");
        let command_name = command_path
            .file_stem()
            .and_then(|name| name.to_str())
            .expect("scratch command should have a UTF-8 stem")
            .to_string();
        let search_directory = command_path
            .parent()
            .expect("scratch file should have parent")
            .to_path_buf();
        let _guard = ScratchFile::new(
            command_path.clone(),
            "@echo off\r\necho envoy executor test\r\n",
        );

        let resolved = ProcessExecutor::resolve_executable(
            Path::new(&command_name),
            Some(search_directory.to_string_lossy().as_ref()),
        )
        .expect("PATHEXT resolution should find .cmd file");

        assert!(resolved
            .to_string_lossy()
            .eq_ignore_ascii_case(&command_path.to_string_lossy()));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn prepare_command_wraps_batch_scripts_with_cmd() {
        let script_path = unique_test_path("envoy_prepare_command_test.cmd");
        let _guard = ScratchFile::new(
            script_path.clone(),
            "@echo off\r\necho prepare command test\r\n",
        );
        let executor = ProcessExecutor::new(false);
        let args = vec![String::from("first"), String::from("second")];

        let command = executor
            .prepare_command(script_path.as_path(), &args, None)
            .expect("batch script should prepare");

        assert_eq!(command[0], "cmd");
        assert_eq!(command[1], "/c");
        assert_eq!(
            PathBuf::from(&command[2]),
            ProcessExecutor::resolve_executable(script_path.as_path(), None)
                .expect("script path should resolve"),
        );
        assert_eq!(command[3], "first");
        assert_eq!(command[4], "second");
    }

    #[test]
    fn stream_process_output_captures_lines_and_invokes_callbacks() {
        let stdout_lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let stderr_lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let on_output_lines = Arc::clone(&stdout_lines);
        let on_error_lines = Arc::clone(&stderr_lines);
        let executor = ProcessExecutor::new(false)
            .with_on_output(move |line| {
                on_output_lines
                    .lock()
                    .expect("stdout callback mutex")
                    .push(line.to_string());
            })
            .with_on_error(move |line| {
                on_error_lines
                    .lock()
                    .expect("stderr callback mutex")
                    .push(line.to_string());
            });
        let mut child = stdout_stderr_command()
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn stdout/stderr child");

        let (stdout, stderr) = executor
            .stream_process_output(&mut child)
            .expect("output streaming should succeed");
        let status = child.wait().expect("wait for child");

        assert!(status.success());
        assert_eq!(stdout, "line1\nline2");
        assert_eq!(stderr, "err1");
        assert_eq!(
            stdout_lines
                .lock()
                .expect("stdout callback contents")
                .as_slice(),
            ["line1", "line2"]
        );
        assert_eq!(
            stderr_lines
                .lock()
                .expect("stderr callback contents")
                .as_slice(),
            ["err1"]
        );
    }

    #[test]
    fn terminate_process_stops_long_running_child() {
        let mut child = long_running_command()
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn long-running child");

        ProcessExecutor::terminate_process(Some(&mut child));

        let status = child.try_wait().expect("query child status");

        assert!(status.is_some());
    }

    #[test]
    fn terminate_process_accepts_none() {
        ProcessExecutor::terminate_process(None);
    }

    #[cfg(target_os = "windows")]
    fn stdout_stderr_command() -> Command {
        let mut command = Command::new("cmd");
        command.args(["/C", "echo line1 & echo line2 & echo err1 1>&2"]);
        command
    }

    #[cfg(not(target_os = "windows"))]
    fn stdout_stderr_command() -> Command {
        let mut command = Command::new("sh");
        command.args(["-c", "printf 'line1\\nline2\\n'; printf 'err1\\n' >&2"]);
        command
    }

    #[cfg(target_os = "windows")]
    fn long_running_command() -> Command {
        let mut command = Command::new("cmd");
        command.args(["/C", "ping -n 30 127.0.0.1 >NUL"]);
        command
    }

    #[cfg(not(target_os = "windows"))]
    fn long_running_command() -> Command {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 30"]);
        command
    }

    fn unique_test_path(file_name: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after Unix epoch")
            .as_nanos();
        let test_name = format!(
            "envoy_executor_{}_{}_{}",
            std::process::id(),
            timestamp,
            file_name
        );

        std::env::current_dir()
            .expect("current directory should resolve")
            .join(test_name)
    }

    struct ScratchFile {
        path: PathBuf,
    }

    impl ScratchFile {
        fn new(path: PathBuf, contents: &str) -> Self {
            fs::write(&path, contents).expect("write scratch file");
            Self { path }
        }
    }

    impl Drop for ScratchFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }
}
