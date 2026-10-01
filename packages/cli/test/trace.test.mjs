// `chain inspect --trace`: the page scripts run in a stand-in page, and the
// report merges page and native records. Runs on the built output:
// `npm run build && npm test`.

import assert from "node:assert/strict";
import { test } from "node:test";
import vm from "node:vm";

import { buildReport, dumpScript, formatReport, startScript, stopScript } from "../dist/trace.js";

function fakePage() {
  let now = 0;
  let pending = null;
  const listeners = {};
  const page = {
    performance: { now: () => now },
    requestAnimationFrame: (fn) => ((pending = fn), 1),
    cancelAnimationFrame: () => (pending = null),
    document: {
      visibilityState: "visible",
      addEventListener: (type, fn) => (listeners[type] = fn),
      removeEventListener: (type) => delete listeners[type]
    }
  };
  page.window = page;
  const context = vm.createContext(page);
  return {
    run: (code) => vm.runInContext(code, context),
    frameAt(ms) {
      now = ms;
      const fn = pending;
      pending = null;
      fn?.(ms);
    },
    hide(ms) {
      now = ms;
      page.document.visibilityState = "hidden";
      listeners.visibilitychange?.();
    },
    show(ms) {
      now = ms;
      page.document.visibilityState = "visible";
      listeners.visibilitychange?.();
    },
    get trace() {
      return page.__chainTrace;
    }
  };
}

test("the page script counts frame gaps and skips hidden stretches", () => {
  const page = fakePage();
  assert.equal(JSON.stringify(page.run(startScript)), '{"visibility":"visible","longTaskApi":false}');
  for (const ms of [16, 32, 48, 100, 116]) page.frameAt(ms);
  page.hide(120);
  page.show(5000);
  page.frameAt(5016);
  page.frameAt(5032);

  const dump = page.run(dumpScript);
  assert.equal(dump.frames, 7);
  assert.equal(dump.over50, 1, "the 52 ms gap; the hidden stretch isn't a gap");
  assert.equal(dump.longGaps[0].ms, 52);
  assert.equal(JSON.stringify(dump.visibility.map((v) => v.state)), '["visible","hidden","visible"]');

  assert.ok(page.run(stopScript));
  assert.equal(page.trace, undefined);
  assert.equal(page.run(dumpScript), null);
});

test("the report joins page and native calls by id and groups them", () => {
  const page = {
    durationMs: 1000,
    calls: [
      { id: 1, cmd: "storage_query", summary: "SELECT * FROM course", atMs: 10, jsMs: 4, requestSize: 40, responseSize: 900 },
      { id: 2, cmd: "storage_query", summary: "SELECT * FROM course", atMs: 60, jsMs: 2, requestSize: 40, responseSize: 900 },
      { id: 3, cmd: "http_request", summary: "GET https://x", atMs: 100, jsMs: 80, requestSize: 60, responseSize: 10, error: "boom" }
    ],
    frames: 50,
    over25: 2,
    over34: 1,
    over50: 1,
    longGaps: [{ atMs: 8, ms: 60 }],
    longTasks: [],
    visibility: [{ atMs: 0, state: "visible" }],
    longTaskApi: false
  };
  const native = {
    calls: [
      { id: 1, cmd: "storage_query", atMs: 11, handlerMs: 1.5, sql: [{ sql: "SELECT * FROM course", ms: 1, rows: 12 }] },
      { id: 2, cmd: "storage_query", atMs: 61, handlerMs: 0.5, sql: [{ sql: "SELECT * FROM course", ms: 0.25, rows: 12 }] },
      { id: null, cmd: "plugin:opener|open_url", atMs: 200, handlerMs: 3, sql: [] }
    ],
    samples: [
      { atMs: 250, appRss: 100 * 1024 * 1024, appCpu: 10, webviewRss: null, webviewCpu: null },
      { atMs: 500, appRss: 200 * 1024 * 1024, appCpu: 30, webviewRss: null, webviewCpu: null }
    ]
  };

  const report = buildReport(page, native);
  assert.equal(report.groups[0].key, "http_request: GET https://x", "slowest total first");
  assert.equal(report.groups[0].errors, 1);
  const query = report.groups[1];
  assert.deepEqual(
    { count: query.count, totalMs: query.totalMs, nativeMs: query.nativeMs, sqliteMs: query.sqliteMs, rows: query.rows },
    { count: 2, totalMs: 6, nativeMs: 2, sqliteMs: 1.25, rows: 24 }
  );
  assert.equal(report.calls[2].nativeMs, undefined, "the app never saw call 3");
  assert.deepEqual(report.frames.longGaps[0].inFlight, ["storage_query: SELECT * FROM course"]);
  assert.equal(report.frames.longTasks, null, "no long-task API in this page");
  assert.equal(report.otherNativeCalls.length, 1);
  assert.deepEqual(report.process.app, { peakMb: 200, avgMb: 150, avgCpu: 20, peakCpu: 30 });
  assert.equal(report.process.webview, null);

  const text = formatReport(report);
  assert.match(text, /2 +6\.0 .*storage_query: SELECT \* FROM course/);
  assert.match(text, /\(1 failed\)/);
  assert.match(text, /app process: 150 MB avg, 200 MB peak/);
});
