use std::path::PathBuf;
use std::time::Duration;

use crate::error::{ActionbookError, Result};

/// Base directory for daemon state files.
fn daemons_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".actionbook")
        .join("daemons")
}

/// Return the Unix Domain Socket path for a profile.
pub fn socket_path(profile: &str) -> PathBuf {
    daemons_dir().join(format!("{}.sock", profile))
}

/// Return the PID file path for a profile.
pub fn pid_path(profile: &str) -> PathBuf {
    daemons_dir().join(format!("{}.pid", profile))
}

/// Check whether the daemon for the given profile is alive.
///
/// 1. Read PID file → check process alive via `kill(pid, 0)`.
/// 2. Try connecting to the UDS socket.
pub async fn is_daemon_alive(profile: &str) -> bool {
    let sock = socket_path(profile);
    if !sock.exists() {
        return false;
    }

    // Try connecting to the socket
    match tokio::net::UnixStream::connect(&sock).await {
        Ok(_stream) => true,
        Err(_) => {
            // Socket file exists but no one is listening — check PID
            if let Some(pid) = read_pid(profile) {
                is_pid_alive(pid)
            } else {
                false
            }
        }
    }
}

/// Ensure the daemon for the given profile is running.
///
/// Returns `true` if this call spawned a new daemon, `false` if it was already running.
pub async fn ensure_daemon(profile: &str) -> Result<bool> {
    if is_daemon_alive(profile).await {
        tracing::debug!("Daemon for profile '{}' is already running", profile);
        return Ok(false);
    }

    // Clean up stale files
    cleanup_files(profile);

    // Spawn daemon
    let exe = std::env::current_exe().map_err(|e| {
        ActionbookError::DaemonError(format!("Cannot determine actionbook binary path: {}", e))
    })?;

    tracing::info!("Auto-starting daemon for profile '{}' ...", profile);
    spawn_detached(&exe, profile)?;

    // Poll UDS until daemon is reachable (up to 5 seconds)
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if is_daemon_alive(profile).await {
            tracing::info!("Daemon for profile '{}' is now running", profile);
            return Ok(true);
        }
    }

    Err(ActionbookError::DaemonError(format!(
        "Daemon for profile '{}' did not start within 5 seconds",
        profile
    )))
}

/// Stop the daemon for the given profile.
pub async fn stop_daemon(profile: &str) -> Result<()> {
    let pid = match read_pid(profile) {
        Some(pid) => pid,
        None => {
            // No PID file — check if socket exists and try to infer state
            if !socket_path(profile).exists() {
                return Ok(()); // Nothing to stop
            }
            cleanup_files(profile);
            return Ok(());
        }
    };

    // Guard: PID must be positive and fit in i32
    if pid == 0 || pid > i32::MAX as u32 {
        tracing::warn!("Invalid PID {} in daemon PID file, cleaning up", pid);
        cleanup_files(profile);
        return Ok(());
    }

    if !is_pid_alive(pid) {
        cleanup_files(profile);
        return Ok(());
    }

    // Send SIGTERM
    #[cfg(unix)]
    {
        let result = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
        if result != 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::ESRCH) {
                cleanup_files(profile);
                return Ok(());
            }
            return Err(ActionbookError::DaemonError(format!(
                "Failed to send SIGTERM to daemon PID {}: {}",
                pid, err
            )));
        }
    }

    #[cfg(not(unix))]
    {
        let status = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string()])
            .status();
        if !matches!(status, Ok(s) if s.success()) {
            if !is_pid_alive(pid) {
                cleanup_files(profile);
                return Ok(());
            }
            return Err(ActionbookError::DaemonError(format!(
                "Failed to terminate daemon PID {}",
                pid
            )));
        }
    }

    // Wait for graceful exit
    tokio::time::sleep(Duration::from_millis(500)).await;

    if !is_pid_alive(pid) {
        cleanup_files(profile);
        tracing::info!("Daemon for profile '{}' stopped (PID {})", profile, pid);
        return Ok(());
    }

    // Escalate to SIGKILL if still alive
    #[cfg(unix)]
    {
        tokio::time::sleep(Duration::from_secs(2)).await;
        if is_pid_alive(pid) {
            unsafe { libc::kill(pid as i32, libc::SIGKILL) };
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    // Only clean up files if the process is actually dead
    if is_pid_alive(pid) {
        return Err(ActionbookError::DaemonError(format!(
            "Daemon PID {} did not terminate after SIGKILL",
            pid
        )));
    }

    cleanup_files(profile);
    tracing::info!("Daemon for profile '{}' stopped (PID {})", profile, pid);
    Ok(())
}

/// Spawn `actionbook daemon serve --profile <profile>` as a fully detached background process.
fn spawn_detached(exe: &std::path::Path, profile: &str) -> Result<()> {
    use std::process::{Command, Stdio};

    let mut cmd = Command::new(exe);
    cmd.args(["daemon", "serve", "--profile", profile])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // On Unix, use setsid + pre_exec to fully detach from the parent process group
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: setsid() is async-signal-safe and called between fork and exec
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    let child = cmd.spawn().map_err(|e| {
        ActionbookError::DaemonError(format!(
            "Failed to spawn daemon process: {}. Binary: {}",
            e,
            exe.display()
        ))
    })?;

    tracing::debug!("Spawned daemon process PID={}", child.id());
    drop(child);
    Ok(())
}

/// Write PID file for the daemon.
pub fn write_pid(profile: &str, pid: u32) -> Result<()> {
    let dir = daemons_dir();
    std::fs::create_dir_all(&dir)?;
    std::fs::write(pid_path(profile), pid.to_string())?;
    Ok(())
}

/// Read PID from the PID file for the daemon.
fn read_pid(profile: &str) -> Option<u32> {
    let path = pid_path(profile);
    std::fs::read_to_string(path)
        .ok()?
        .trim()
        .parse::<u32>()
        .ok()
}

/// Check if a PID is alive.
fn is_pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        // On Windows, use tasklist
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid)])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    }
}

/// Clean up socket and PID files for a profile.
pub fn cleanup_files(profile: &str) {
    let _ = std::fs::remove_file(socket_path(profile));
    let _ = std::fs::remove_file(pid_path(profile));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_path_format() {
        let path = socket_path("default");
        assert!(path.to_string_lossy().ends_with("daemons/default.sock"));
    }

    #[test]
    fn pid_path_format() {
        let path = pid_path("my-profile");
        assert!(path.to_string_lossy().ends_with("daemons/my-profile.pid"));
    }

    #[tokio::test]
    async fn is_daemon_alive_returns_false_when_no_socket() {
        assert!(!is_daemon_alive("nonexistent-profile-12345").await);
    }
}
