import { invoke, isTauri } from "@tauri-apps/api/core";

import type { StorageApi, StorageScope, Migration, ExecuteResult } from "./contracts/storage";
import { chainError } from "./errors";
import { Table, type Runner } from "./storage-table";

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) {
    throw chainError(
      "UNSUPPORTED",
      "desktop.storage requires running inside a Chain (Tauri) app — it can't work in a plain Node/browser context"
    );
  }
  try {
    return await invoke<T>(cmd, args);
  } catch (error) {
    throw chainError(
      "NATIVE_FAILURE",
      typeof error === "string" ? error : "storage operation failed"
    );
  }
}

// Storage calls run one at a time, in call order, so a transaction holds the
// line until it settles. A call queued behind an open transaction gives up
// after a while: most likely it was made inside the transaction's own
// callback, where it would otherwise wait forever.
const TRANSACTION_WAIT_MS = 10_000;
let line: Promise<unknown> = Promise.resolve();
let transactionOpen = false;

function inLine<T>(run: () => Promise<T>): Promise<T> {
  const behindTransaction = transactionOpen;
  let started = false;
  let gaveUp = false;
  const turn = line.then(() => {
    if (gaveUp) throw chainError("UNAVAILABLE", "gave up waiting on a transaction");
    started = true;
    return run();
  });
  line = turn.catch(() => undefined);
  if (!behindTransaction) return turn;

  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => {
      if (started) return;
      gaveUp = true;
      reject(
        chainError(
          "UNAVAILABLE",
          `desktop.storage waited ${TRANSACTION_WAIT_MS / 1000} s for an open transaction — inside transaction(), use the tx it passes, not desktop.storage`
        )
      );
    }, TRANSACTION_WAIT_MS);
  });
  return Promise.race([turn, timeout]).finally(() => clearTimeout(timer));
}

function scope(runner: Runner): StorageScope {
  return {
    query: <T>(sql: string, params: unknown[] = []) => runner.query<T>(sql, params),
    execute: (sql: string, params: unknown[] = []) => runner.execute(sql, params),
    table: <T>(name: string) => new Table<T>(runner, name)
  };
}

function inTransaction(transaction: number): Runner {
  return {
    query: (sql, params) => call("storage_query", { transaction, sql, params }),
    execute: (sql, params) => call("storage_execute", { transaction, sql, params })
  };
}

export const storage: StorageApi = {
  ...scope({
    query: (sql, params) => inLine(() => call("storage_query", { sql, params })),
    execute: (sql, params) => inLine(() => call<ExecuteResult>("storage_execute", { sql, params }))
  }),
  migrate(migrations: Migration[]): Promise<void> {
    return inLine(() => call("storage_migrate", { migrations }));
  },
  transaction<R>(work: (tx: StorageScope) => Promise<R>): Promise<R> {
    return inLine(async () => {
      const transaction = await call<number>("storage_begin");
      transactionOpen = true;
      try {
        const result = await work(scope(inTransaction(transaction)));
        await call("storage_commit", { transaction });
        return result;
      } catch (error) {
        await call("storage_rollback", { transaction }).catch(() => undefined);
        throw error;
      } finally {
        transactionOpen = false;
      }
    });
  }
};
