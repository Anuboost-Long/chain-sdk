// `chain inspect --trace` — the page half (scripts eval'd into the webview)
// and the report built from both halves. The SDK records every native call
// into the page's `window.__chainTrace` (packages/sdk/src/native.ts); the
// app records the same calls by id, plus SQLite statements and process
// samples (crates/core/src/dev_trace.rs). See the inspector section of
// agent-docs/framework/command/README.md.

/** Gaps between frames longer than this are kept individually, with the calls in flight during them. */
const LONG_GAP_MS = 50;
const MAX_LONG_GAPS = 500;

// Started fresh: replaces a running trace. rAF stops while the page is
// hidden, so a hidden stretch is recorded as such instead of as one huge gap.
// The first bucket is 25 ms, not 16.7: WKWebView rounds performance.now() to
// whole milliseconds, so an on-time 60 Hz frame often measures 17 ms.
export const startScript = `(function () {
  var old = window.__chainTrace;
  if (old && old.stopFrames) old.stopFrames();
  var t = { startedAt: performance.now(), nextId: 0, calls: [], frames: 0,
    over25: 0, over34: 0, over50: 0, longGaps: [], longTasks: [], visibility: [], longTaskApi: false };
  var last = null, raf = 0, observer = null;
  function frame(now) {
    if (last !== null) {
      var gap = now - last;
      if (gap > 25) t.over25++;
      if (gap > 33.4) t.over34++;
      if (gap > 50) { t.over50++; if (t.longGaps.length < ${MAX_LONG_GAPS}) t.longGaps.push({ atMs: last - t.startedAt, ms: gap }); }
    }
    last = now;
    t.frames++;
    raf = requestAnimationFrame(frame);
  }
  function onVisibility() {
    last = null;
    t.visibility.push({ atMs: performance.now() - t.startedAt, state: document.visibilityState });
  }
  document.addEventListener("visibilitychange", onVisibility);
  if (window.PerformanceObserver && (PerformanceObserver.supportedEntryTypes || []).indexOf("longtask") !== -1) {
    observer = new PerformanceObserver(function (list) {
      list.getEntries().forEach(function (e) { t.longTasks.push({ atMs: e.startTime - t.startedAt, ms: e.duration }); });
    });
    observer.observe({ type: "longtask" });
    t.longTaskApi = true;
  }
  t.stopFrames = function () {
    cancelAnimationFrame(raf);
    if (observer) observer.disconnect();
    document.removeEventListener("visibilitychange", onVisibility);
  };
  t.visibility.push({ atMs: 0, state: document.visibilityState });
  raf = requestAnimationFrame(frame);
  window.__chainTrace = t;
  return { visibility: document.visibilityState, longTaskApi: t.longTaskApi };
})()`;

const snapshot = `{ durationMs: performance.now() - t.startedAt, calls: t.calls, frames: t.frames,
    over25: t.over25, over34: t.over34, over50: t.over50, longGaps: t.longGaps, longTasks: t.longTasks,
    visibility: t.visibility, longTaskApi: t.longTaskApi }`;

export const dumpScript = `(function () {
  var t = window.__chainTrace;
  return t ? ${snapshot} : null;
})()`;

export const stopScript = `(function () {
  var t = window.__chainTrace;
  if (!t) return null;
  t.stopFrames();
  delete window.__chainTrace;
  return ${snapshot};
})()`;

interface PageCall {
  id: number;
  cmd: string;
  summary: string;
  atMs: number;
  jsMs: number;
  requestSize: number;
  responseSize: number;
  error?: string;
}

interface Span {
  atMs: number;
  ms: number;
}

export interface PageTrace {
  durationMs: number;
  calls: PageCall[];
  frames: number;
  over25: number;
  over34: number;
  over50: number;
  longGaps: Span[];
  longTasks: Span[];
  visibility: { atMs: number; state: string }[];
  longTaskApi: boolean;
}

interface SqlStatement {
  sql: string;
  ms: number;
  rows: number;
}

interface NativeCall {
  id: number | null;
  cmd: string;
  atMs: number;
  handlerMs: number;
  sql: SqlStatement[];
}

interface ProcessSample {
  atMs: number;
  appRss: number;
  appCpu: number;
  webviewRss: number | null;
  webviewCpu: number | null;
}

export interface NativeTrace {
  calls: NativeCall[];
  samples: ProcessSample[];
}

export interface TracedCall extends PageCall {
  /** Handler time in the app; for an async command, only its dispatch. Absent if the app didn't see the call. */
  nativeMs?: number;
  sqliteMs?: number;
  rows?: number;
  sql?: SqlStatement[];
}

interface Group {
  key: string;
  count: number;
  totalMs: number;
  medianMs: number;
  p95Ms: number;
  nativeMs: number;
  sqliteMs: number;
  rows: number;
  errors: number;
}

interface Usage {
  peakMb: number;
  avgMb: number;
  avgCpu: number;
  peakCpu: number;
}

export interface TraceReport {
  durationMs: number;
  groups: Group[];
  calls: TracedCall[];
  /** Commands the app ran that the SDK didn't make. */
  otherNativeCalls: NativeCall[];
  frames: {
    count: number;
    /** A dropped frame at 60 Hz. */
    over25ms: number;
    over33ms: number;
    over50ms: number;
    longGaps: (Span & { inFlight: string[] })[];
    longTasks: Span[] | null;
    visibility: { atMs: number; state: string }[];
  };
  process: { samples: ProcessSample[]; app: Usage | null; webview: Usage | null };
}

function percentile(sorted: number[], p: number): number {
  if (!sorted.length) return 0;
  return sorted[Math.min(sorted.length - 1, Math.ceil((p / 100) * sorted.length) - 1)];
}

const sum = (values: number[]) => values.reduce((a, b) => a + b, 0);

function usage(samples: { rss: number; cpu: number }[]): Usage | null {
  if (!samples.length) return null;
  const mb = (bytes: number) => Math.round((bytes / 1024 / 1024) * 10) / 10;
  return {
    peakMb: mb(Math.max(...samples.map((s) => s.rss))),
    avgMb: mb(sum(samples.map((s) => s.rss)) / samples.length),
    avgCpu: Math.round((sum(samples.map((s) => s.cpu)) / samples.length) * 10) / 10,
    peakCpu: Math.round(Math.max(...samples.map((s) => s.cpu)) * 10) / 10
  };
}

const groupKey = (call: PageCall) => (call.summary ? `${call.cmd}: ${call.summary}` : call.cmd);

export function buildReport(page: PageTrace, native: NativeTrace | null): TraceReport {
  const byId = new Map((native?.calls ?? []).filter((c) => c.id !== null).map((c) => [c.id, c]));
  const calls: TracedCall[] = page.calls.map((call) => {
    const seen = byId.get(call.id);
    if (!seen) return call;
    const traced: TracedCall = { ...call, nativeMs: seen.handlerMs };
    if (seen.sql.length) {
      traced.sql = seen.sql;
      traced.sqliteMs = sum(seen.sql.map((s) => s.ms));
      traced.rows = sum(seen.sql.map((s) => s.rows));
    }
    return traced;
  });

  const grouped = new Map<string, TracedCall[]>();
  for (const call of calls) grouped.set(groupKey(call), [...(grouped.get(groupKey(call)) ?? []), call]);
  const groups = [...grouped].map(([key, members]): Group => {
    const times = members.map((c) => c.jsMs).sort((a, b) => a - b);
    return {
      key,
      count: members.length,
      totalMs: sum(times),
      medianMs: percentile(times, 50),
      p95Ms: percentile(times, 95),
      nativeMs: sum(members.map((c) => c.nativeMs ?? 0)),
      sqliteMs: sum(members.map((c) => c.sqliteMs ?? 0)),
      rows: sum(members.map((c) => c.rows ?? 0)),
      errors: members.filter((c) => c.error !== undefined).length
    };
  });
  groups.sort((a, b) => b.totalMs - a.totalMs);

  const inFlight = (gap: Span) => [
    ...new Set(calls.filter((c) => c.atMs < gap.atMs + gap.ms && c.atMs + c.jsMs > gap.atMs).map(groupKey))
  ];

  const samples = native?.samples ?? [];
  const webviewSamples = samples.filter((s) => s.webviewRss !== null);
  return {
    durationMs: page.durationMs,
    groups,
    calls,
    otherNativeCalls: (native?.calls ?? []).filter((c) => c.id === null),
    frames: {
      count: page.frames,
      over25ms: page.over25,
      over33ms: page.over34,
      over50ms: page.over50,
      longGaps: page.longGaps.map((gap) => ({ ...gap, inFlight: inFlight(gap) })),
      longTasks: page.longTaskApi ? page.longTasks : null,
      visibility: page.visibility
    },
    process: {
      samples,
      app: usage(samples.map((s) => ({ rss: s.appRss, cpu: s.appCpu }))),
      webview: usage(webviewSamples.map((s) => ({ rss: s.webviewRss ?? 0, cpu: s.webviewCpu ?? 0 })))
    }
  };
}

const fixed = (ms: number) => ms.toFixed(1);

/** The report as text: calls grouped by command or SQL, then frames, then process usage. */
export function formatReport(report: TraceReport): string {
  const lines = [`trace: ${fixed(report.durationMs / 1000)} s, ${report.calls.length} native calls`, ""];
  lines.push("count  total ms  median  p95     native  sqlite  rows    call");
  for (const g of report.groups) {
    const cells = [g.count, fixed(g.totalMs), fixed(g.medianMs), fixed(g.p95Ms), fixed(g.nativeMs), fixed(g.sqliteMs), g.rows];
    const widths = [5, 8, 6, 6, 6, 6, 6];
    const row = cells.map((c, i) => String(c).padStart(widths[i])).join("  ");
    const key = g.key.length > 100 ? g.key.slice(0, 99) + "…" : g.key;
    const failed = g.errors ? "  (" + g.errors + " failed)" : "";
    lines.push(`${row}  ${key}${failed}`);
  }

  const { frames } = report;
  lines.push(
    "",
    `frames: ${frames.count}; gaps over 25 ms: ${frames.over25ms}, over 33 ms: ${frames.over33ms}, over 50 ms: ${frames.over50ms}`
  );
  for (const gap of frames.longGaps.slice(0, 20)) {
    const during = gap.inFlight.length ? ` during ${gap.inFlight.slice(0, 3).join(", ")}` : "";
    lines.push(`  ${fixed(gap.ms)} ms at ${fixed(gap.atMs)} ms${during}`);
  }
  if (frames.longTasks) lines.push(`long tasks: ${frames.longTasks.length}`);
  if (frames.visibility.some((v) => v.state !== "visible")) {
    lines.push("note: the page was hidden for part of the trace, and hidden pages don't render frames — run --focus first");
  }

  const { app, webview } = report.process;
  const describe = (name: string, u: Usage | null) =>
    u ? `${name}: ${u.avgMb} MB avg, ${u.peakMb} MB peak; CPU ${u.avgCpu}% avg, ${u.peakCpu}% peak` : `${name}: no samples`;
  lines.push("", describe("app process", app), describe("webview", webview));
  if (report.otherNativeCalls.length) lines.push(`other native calls (not from the SDK): ${report.otherNativeCalls.length}`);
  return lines.join("\n");
}
