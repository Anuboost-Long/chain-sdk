//! Stopping a spawned process together with everything it started, shared
//! by process_runner and terminal. On Unix each spawned process leads its
//! own process group, so the group is signalled as a whole; on Windows
//! `taskkill /T` walks the tree. See
//! agent-docs/capabilities/process-runner/CONTRACT.md's `kill()`.

use std::time::Duration;

/// SIGTERM, then SIGKILL after this long — the same escalation Lazify's
/// Electron runner used.
pub const GRACE: Duration = Duration::from_secs(2);
/// Give up (and report it) if the tree is still alive after this long.
pub const LIMIT: Duration = Duration::from_secs(5);

/// Makes the spawned process the leader of a new process group, so
/// `terminate` can reach its children too.
pub fn lead_own_group(command: &mut std::process::Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(not(unix))]
    let _ = command;
}

/// Stops `pid`'s whole tree: SIGTERM to its group, SIGKILL after `grace`,
/// then waits until every member is gone. `Err` if some are still alive at
/// `limit`. Stopping a tree that is already gone succeeds.
#[cfg(unix)]
pub fn terminate(pid: u32, grace: Duration, limit: Duration) -> Result<(), String> {
    let group = pid as libc::pid_t;
    let alive = || unsafe { libc::killpg(group, 0) } == 0;
    let started = std::time::Instant::now();
    let mut signal = libc::SIGTERM;
    unsafe { libc::killpg(group, signal) };
    while alive() {
        if signal == libc::SIGTERM && started.elapsed() >= grace {
            signal = libc::SIGKILL;
            unsafe { libc::killpg(group, signal) };
        }
        if started.elapsed() >= limit {
            return Err(format!("process group {pid} was still running {}s after it was told to stop", limit.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

/// No grace period: `taskkill /F` is the only reliable tree-wide stop
/// without a job object. Unverified — see process-runner/research/WINDOWS.md.
#[cfg(windows)]
pub fn terminate(pid: u32, _grace: Duration, _limit: Duration) -> Result<(), String> {
    let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    let taskkill = std::path::Path::new(&system_root).join("System32").join("taskkill.exe");
    let output = std::process::Command::new(taskkill)
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .output()
        .map_err(|e| e.to_string())?;
    // 128: no such process — already gone, which is success.
    if output.status.success() || output.status.code() == Some(128) {
        return Ok(());
    }
    Err(String::from_utf8_lossy(&output.stderr).into_owned())
}

/// Immediate, no waiting — for the app quitting.
pub fn kill_now(pid: u32) {
    #[cfg(unix)]
    unsafe {
        libc::killpg(pid as libc::pid_t, libc::SIGKILL);
    }
    #[cfg(windows)]
    let _ = terminate(pid, Duration::ZERO, Duration::ZERO);
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};
    use std::time::Instant;

    fn alive(pid: u32) -> bool {
        unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
    }

    #[test]
    fn stops_the_children_a_wrapper_started_not_just_the_wrapper() {
        // The wrapper starts a long-lived child and prints its pid, like
        // `npm run dev` starting the real dev server.
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "sleep 30 & echo $!; wait"]).stdout(Stdio::piped());
        lead_own_group(&mut command);
        let mut wrapper = command.spawn().unwrap();
        let mut line = String::new();
        std::io::BufRead::read_line(&mut std::io::BufReader::new(wrapper.stdout.take().unwrap()), &mut line).unwrap();
        let grandchild: u32 = line.trim().parse().unwrap();
        assert!(alive(grandchild));

        let pid = wrapper.id();
        let reaper = std::thread::spawn(move || wrapper.wait());
        terminate(pid, GRACE, LIMIT).unwrap();
        reaper.join().unwrap().unwrap();
        assert!(!alive(grandchild), "the grandchild survived");
    }

    #[test]
    fn escalates_to_sigkill_when_sigterm_is_ignored() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "trap '' TERM; sleep 30"]);
        lead_own_group(&mut command);
        let mut child = command.spawn().unwrap();
        let pid = child.id();
        let reaper = std::thread::spawn(move || child.wait());
        std::thread::sleep(Duration::from_millis(100));

        let started = Instant::now();
        terminate(pid, Duration::from_millis(300), LIMIT).unwrap();
        assert!(started.elapsed() >= Duration::from_millis(300));
        assert!(started.elapsed() < Duration::from_secs(3));
        reaper.join().unwrap().unwrap();
    }

    #[test]
    fn stopping_a_tree_that_is_gone_succeeds() {
        let mut command = Command::new("/usr/bin/true");
        lead_own_group(&mut command);
        let mut child = command.spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        terminate(pid, GRACE, LIMIT).unwrap();
    }
}
