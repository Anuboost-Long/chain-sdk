//! Native half of `chain inspect --trace` — see the inspector section of
//! agent-docs/framework/command/README.md. Records each traced command's
//! handler time, the SQL storage ran inside it, and app/webview process
//! samples. Only built for real with the `dev-trace` feature, which only
//! `chain dev` passes; without it `record_sql` is the one item left, an
//! empty stub, so release builds record nothing.

#[cfg(not(feature = "dev-trace"))]
pub fn record_sql(_sql: &str, _started: std::time::Instant, _rows: usize) {}

#[cfg(feature = "dev-trace")]
pub use enabled::*;

#[cfg(feature = "dev-trace")]
mod enabled {
    use serde::Serialize;
    use std::cell::Cell;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

    const SAMPLE_EVERY: Duration = Duration::from_millis(250);

    static ACTIVE: AtomicBool = AtomicBool::new(false);
    /// Bumped by every `start`, so a previous trace's sampler thread stops.
    static GENERATION: AtomicU64 = AtomicU64::new(0);
    static TRACE: Mutex<Option<Trace>> = Mutex::new(None);

    thread_local! {
        /// The call whose handler is running on this thread — where `record_sql` files its statement.
        static CURRENT: Cell<Option<usize>> = const { Cell::new(None) };
    }

    struct Trace {
        started: Instant,
        calls: Vec<NativeCall>,
        samples: Vec<ProcessSample>,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct NativeCall {
        /// The SDK's id for the call (its `chain-trace` header); absent for calls the SDK didn't make.
        id: Option<u64>,
        cmd: String,
        at_ms: f64,
        /// Time inside the command handler. An async command returns here once it's
        /// started, so this is only its dispatch time, not its work.
        handler_ms: f64,
        sql: Vec<SqlStatement>,
    }

    #[derive(Serialize)]
    struct SqlStatement {
        sql: String,
        ms: f64,
        rows: usize,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct ProcessSample {
        at_ms: f64,
        app_rss: u64,
        app_cpu: f32,
        /// Summed over the webview's processes; absent when none were found.
        webview_rss: Option<u64>,
        webview_cpu: Option<f32>,
    }

    fn ms(duration: Duration) -> f64 {
        duration.as_secs_f64() * 1000.0
    }

    fn with_trace<T>(f: impl FnOnce(&mut Trace) -> T) -> Option<T> {
        TRACE.lock().expect("trace mutex poisoned").as_mut().map(f)
    }

    /// Starts a fresh trace, discarding the last one. `webview_pids` are the
    /// webview's own processes where they aren't this process's children
    /// (macOS's WebContent); elsewhere they're found as descendants.
    pub fn start(webview_pids: Vec<u32>) {
        *TRACE.lock().expect("trace mutex poisoned") =
            Some(Trace { started: Instant::now(), calls: Vec::new(), samples: Vec::new() });
        ACTIVE.store(true, Ordering::SeqCst);
        let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        std::thread::spawn(move || sample_processes(generation, webview_pids));
    }

    pub fn stop() {
        ACTIVE.store(false, Ordering::SeqCst);
    }

    pub fn dump() -> serde_json::Value {
        with_trace(|trace| serde_json::json!({ "calls": trace.calls, "samples": trace.samples }))
            .unwrap_or(serde_json::Value::Null)
    }

    /// Runs one command's handler, recording it when a trace is on.
    pub fn command<T>(cmd: &str, id: Option<u64>, run: impl FnOnce() -> T) -> T {
        if !ACTIVE.load(Ordering::Relaxed) {
            return run();
        }
        let started = Instant::now();
        let index = with_trace(|trace| {
            trace.calls.push(NativeCall {
                id,
                cmd: cmd.to_string(),
                at_ms: ms(started - trace.started),
                handler_ms: 0.0,
                sql: Vec::new(),
            });
            trace.calls.len() - 1
        });
        CURRENT.set(index);
        let out = run();
        CURRENT.set(None);
        if let Some(index) = index {
            with_trace(|trace| {
                if let Some(call) = trace.calls.get_mut(index) {
                    call.handler_ms = ms(started.elapsed());
                }
            });
        }
        out
    }

    pub fn record_sql(sql: &str, started: Instant, rows: usize) {
        let Some(index) = CURRENT.get() else { return };
        let elapsed = ms(started.elapsed());
        with_trace(|trace| {
            if let Some(call) = trace.calls.get_mut(index) {
                call.sql.push(SqlStatement { sql: sql.split_whitespace().collect::<Vec<_>>().join(" "), ms: elapsed, rows });
            }
        });
    }

    fn descendants(system: &System, root: Pid) -> Vec<Pid> {
        let mut found = vec![root];
        let mut i = 0;
        while i < found.len() {
            let parent = found[i];
            found.extend(system.processes().iter().filter(|(_, p)| p.parent() == Some(parent)).map(|(pid, _)| *pid));
            i += 1;
        }
        found.remove(0);
        found
    }

    fn sample_processes(generation: u64, webview_pids: Vec<u32>) {
        let app = Pid::from_u32(std::process::id());
        let mut system = System::new();
        system.refresh_processes(ProcessesToUpdate::All, true);
        let mut webview: Vec<Pid> = webview_pids.into_iter().map(Pid::from_u32).collect();
        webview.extend(descendants(&system, app));
        let watched: Vec<Pid> = std::iter::once(app).chain(webview.iter().copied()).collect();
        let refresh = ProcessRefreshKind::nothing().with_memory().with_cpu();

        while ACTIVE.load(Ordering::SeqCst) && GENERATION.load(Ordering::SeqCst) == generation {
            std::thread::sleep(SAMPLE_EVERY);
            system.refresh_processes_specifics(ProcessesToUpdate::Some(&watched), true, refresh);
            let Some(app_process) = system.process(app) else { return };
            let views: Vec<_> = webview.iter().filter_map(|pid| system.process(*pid)).collect();
            let sample = ProcessSample {
                at_ms: 0.0,
                app_rss: app_process.memory(),
                app_cpu: app_process.cpu_usage(),
                webview_rss: (!views.is_empty()).then(|| views.iter().map(|p| p.memory()).sum()),
                webview_cpu: (!views.is_empty()).then(|| views.iter().map(|p| p.cpu_usage()).sum()),
            };
            with_trace(|trace| trace.samples.push(ProcessSample { at_ms: ms(trace.started.elapsed()), ..sample }));
        }
    }

    /// The WebContent process behind a WKWebView, through WebKit's
    /// `_webProcessIdentifier` (private, but dev-only here).
    #[cfg(target_os = "macos")]
    pub fn webview_process_id(wk_webview: *mut std::ffi::c_void) -> Option<u32> {
        use objc2::runtime::AnyObject;
        use objc2::{msg_send, sel};
        let webview = unsafe { (wk_webview as *const AnyObject).as_ref()? };
        let responds: bool = unsafe { msg_send![webview, respondsToSelector: sel!(_webProcessIdentifier)] };
        if !responds {
            return None;
        }
        let pid: i32 = unsafe { msg_send![webview, _webProcessIdentifier] };
        u32::try_from(pid).ok().filter(|pid| *pid > 0)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn records_commands_and_their_sql_only_while_tracing() {
            command("before", None, || record_sql("SELECT 0", Instant::now(), 0));
            start(Vec::new());
            command("storage_query", Some(7), || {
                record_sql("SELECT  1\n FROM t", Instant::now(), 3);
            });
            record_sql("SELECT 2", Instant::now(), 1);
            let dump = dump();
            stop();
            let calls = dump["calls"].as_array().unwrap();
            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0]["id"], 7);
            assert_eq!(calls[0]["sql"][0]["sql"], "SELECT 1 FROM t");
            assert_eq!(calls[0]["sql"][0]["rows"], 3);
        }
    }
}
