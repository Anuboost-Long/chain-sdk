/**
 * Structural contract for the Storage capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

export interface Migration {
  version: number;
  sql: string;
}

export interface ExecuteResult {
  rowsAffected: number;
  lastInsertId: number;
}

export interface StorageApi {
  migrate(migrations: Migration[]): Promise<void>;
  query<T = unknown>(sql: string, params?: unknown[]): Promise<T[]>;
  execute(sql: string, params?: unknown[]): Promise<ExecuteResult>;
}
