//! Makes a `chain dev` binary responsible for its own OS permissions.
//!
//! macOS attributes a privacy-gated request (microphone, speech
//! recognition, ...) to the *responsible* process. A binary started by
//! `tauri dev` inherits responsibility from the terminal or editor that ran
//! `chain dev`, so TCC asks about — and silently denies on behalf of — that
//! app instead: no prompt, and the Chain app never appears in System
//! Settings → Privacy & Security. A bundled app launched on its own is
//! responsible for itself, which is why `chain build` output never has this
//! problem.
//!
//! [`become_responsible_for_itself`] fixes the dev case the way terminals
//! and editors do for their own children: it relaunches the binary with
//! responsibility disclaimed (`responsibility_spawnattrs_setdisclaim`), so
//! TCC reads this binary's embedded Info.plist usage descriptions and
//! prompts with its own name. The original process stays as a thin waiter,
//! so `tauri dev`'s pid, exit code, and log streaming behave as before.
//! See agent-docs/framework/command/README.md ("Permission prompts under
//! `chain dev`").
//!
//! Both entry points are private libSystem SPI (used by Chromium, Qt
//! Creator, iTerm2, ...), so they're looked up at runtime: if a future
//! macOS drops them, this degrades to today's behavior instead of failing
//! to launch.

/// Relaunches the current process with responsibility disclaimed and
/// exits with the relaunched copy's status, unless the process is already
/// responsible for itself. Returns normally in the process that should go
/// on to run the app. A no-op everywhere but macOS.
///
/// Call it first thing in `main`/`run`, before any threads or windows.
pub fn become_responsible_for_itself() {
    #[cfg(target_os = "macos")]
    macos::become_responsible_for_itself();
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::{CStr, CString};
    use std::os::unix::ffi::OsStrExt;

    /// Holds the read end of a pipe whose write end only the waiting
    /// parent owns. EOF on it means the parent is gone — `tauri dev` kills
    /// the pid it started (on a Rust change, or quitting), and SIGKILL
    /// can't be forwarded — so the relaunched app exits with it.
    const PARENT_PIPE_ENV: &str = "CHAIN_DEV_PARENT_PIPE";

    type ResponsibleForPid = unsafe extern "C" fn(libc::pid_t) -> libc::pid_t;
    type SetDisclaim = unsafe extern "C" fn(*mut libc::posix_spawnattr_t, libc::c_int) -> libc::c_int;

    fn lookup<T>(name: &CStr) -> Option<T> {
        // SAFETY: RTLD_DEFAULT searches the already-loaded images; callers
        // only transmute to the documented C signature of `name`.
        let symbol = unsafe { libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr()) };
        (!symbol.is_null()).then(|| unsafe { std::mem::transmute_copy(&symbol) })
    }

    pub fn become_responsible_for_itself() {
        if let Some(fd) = std::env::var(PARENT_PIPE_ENV).ok().and_then(|v| v.parse().ok()) {
            std::env::remove_var(PARENT_PIPE_ENV);
            exit_with_parent(fd);
            return;
        }

        let Some(responsible_for) = lookup::<ResponsibleForPid>(c"responsibility_get_pid_responsible_for_pid") else {
            return;
        };
        let Some(set_disclaim) = lookup::<SetDisclaim>(c"responsibility_spawnattrs_setdisclaim") else {
            return;
        };
        // SAFETY: plain syscall wrappers.
        let pid = unsafe { libc::getpid() };
        if unsafe { responsible_for(pid) } == pid {
            return;
        }

        if let Some(status) = relaunch_disclaimed(set_disclaim) {
            std::process::exit(status);
        }
    }

    fn exit_with_parent(fd: libc::c_int) {
        std::thread::spawn(move || {
            let mut byte = 0u8;
            // SAFETY: fd is the inherited pipe read end; the parent never
            // writes, so this returns only when its write end closes.
            while unsafe { libc::read(fd, (&mut byte as *mut u8).cast(), 1) } < 0
                && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted
            {}
            // _exit, not process::exit: atexit handlers racing the main
            // thread's run loop is how a quit turns into a crash report.
            unsafe { libc::_exit(0) };
        });
    }

    /// Spawns this binary again, disclaimed, and waits for it. Returns its
    /// exit status, or None if it couldn't be started (the caller then
    /// just runs the app itself).
    fn relaunch_disclaimed(set_disclaim: SetDisclaim) -> Option<i32> {
        let exe = CString::new(std::env::current_exe().ok()?.as_os_str().as_bytes()).ok()?;
        let args: Vec<CString> = std::env::args_os()
            .map(|arg| CString::new(arg.as_bytes()))
            .collect::<Result<_, _>>()
            .ok()?;

        let mut pipe = [0; 2];
        // SAFETY: pipe fills both fds on success.
        if unsafe { libc::pipe(pipe.as_mut_ptr()) } != 0 {
            return None;
        }
        let [read_end, write_end] = pipe;
        // Only the read end may reach the child; were the write end
        // inherited too, EOF would never arrive.
        unsafe { libc::fcntl(write_end, libc::F_SETFD, libc::FD_CLOEXEC) };

        let mut env: Vec<CString> = std::env::vars_os()
            .filter(|(key, _)| key != PARENT_PIPE_ENV)
            .filter_map(|(key, value)| {
                let mut entry = key.as_bytes().to_vec();
                entry.push(b'=');
                entry.extend_from_slice(value.as_bytes());
                CString::new(entry).ok()
            })
            .collect();
        env.push(CString::new(format!("{PARENT_PIPE_ENV}={read_end}")).ok()?);

        let argv: Vec<*mut libc::c_char> =
            args.iter().map(|a| a.as_ptr().cast_mut()).chain(std::iter::once(std::ptr::null_mut())).collect();
        let envp: Vec<*mut libc::c_char> =
            env.iter().map(|e| e.as_ptr().cast_mut()).chain(std::iter::once(std::ptr::null_mut())).collect();

        let mut child: libc::pid_t = 0;
        // SAFETY: attr is initialized before use and destroyed after;
        // argv/envp are NUL-terminated arrays of live CStrings.
        let spawned = unsafe {
            let mut attr: libc::posix_spawnattr_t = std::ptr::null_mut();
            if libc::posix_spawnattr_init(&mut attr) != 0 {
                libc::close(read_end);
                libc::close(write_end);
                return None;
            }
            let ok = set_disclaim(&mut attr, 1) == 0
                && libc::posix_spawn(&mut child, exe.as_ptr(), std::ptr::null(), &attr, argv.as_ptr(), envp.as_ptr())
                    == 0;
            libc::posix_spawnattr_destroy(&mut attr);
            libc::close(read_end);
            ok
        };
        if !spawned {
            unsafe { libc::close(write_end) };
            return None;
        }

        let mut status = 0;
        // SAFETY: child is the pid posix_spawn just returned.
        while unsafe { libc::waitpid(child, &mut status, 0) } < 0 {
            if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
                return Some(1);
            }
        }
        Some(if libc::WIFEXITED(status) {
            libc::WEXITSTATUS(status)
        } else {
            128 + libc::WTERMSIG(status)
        })
    }
}
