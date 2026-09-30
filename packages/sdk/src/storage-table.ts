import type {
  ExecuteResult,
  Filter,
  Id,
  OrderBy,
  SqlFragment,
  SqlTag,
  StorageTable,
  TableQuery,
  Target,
  Values
} from "./contracts/storage";
import { chainError } from "./errors";

class Fragment implements SqlFragment {
  constructor(
    readonly sql: string,
    readonly params: readonly unknown[] = []
  ) {}
}

function bind(value: unknown): Fragment {
  if (value instanceof Fragment) return value;
  if (Array.isArray(value)) {
    return value.length ? new Fragment(`(${value.map(() => "?").join(", ")})`, value) : new Fragment("(NULL)");
  }
  return new Fragment("?", [value]);
}

export const sql: SqlTag = (strings, ...values) =>
  values.reduce<Fragment>((text, value, i) => {
    const bound = bind(value);
    return new Fragment(text.sql + bound.sql + strings[i + 1], [...text.params, ...bound.params]);
  }, new Fragment(strings[0]));

function quote(identifier: string): string {
  return `"${identifier.replace(/"/g, '""')}"`;
}

function join(fragments: Fragment[], separator: string): Fragment {
  return new Fragment(
    fragments.map((f) => f.sql).join(separator),
    fragments.flatMap((f) => f.params)
  );
}

function conditions(filter: object): Fragment[] {
  if (filter instanceof Fragment) return [new Fragment(`(${filter.sql})`, filter.params)];
  return Object.entries(filter)
    .filter(([, value]) => value !== undefined)
    .map(([column, value]) => {
      if (value === null) return new Fragment(`${quote(column)} IS NULL`);
      if (Array.isArray(value)) return new Fragment(`${quote(column)} IN ${bind(value).sql}`, value);
      return new Fragment(`${quote(column)} = ?`, [value]);
    });
}

function orderTerm(order: string | SqlFragment): Fragment {
  if (order instanceof Fragment) return order;
  const column = (order as string).replace(/ desc$/, "");
  return new Fragment(column === order ? quote(column) : `${quote(column)} DESC`);
}

function assignments(values: object): [string, Fragment][] {
  return Object.entries(values)
    .filter(([, value]) => value !== undefined)
    .map(([column, value]) => [
      quote(column),
      value instanceof Fragment ? new Fragment(`(${value.sql})`, value.params) : new Fragment("?", [value])
    ]);
}

/** Where a table's statements run: `desktop.storage` itself, or a transaction's `tx`. */
export interface Runner {
  query<T>(sql: string, params: unknown[]): Promise<T[]>;
  execute(sql: string, params: unknown[]): Promise<ExecuteResult>;
}

interface QueryState {
  where: Fragment[];
  order: Fragment[];
  limit?: number;
}

class Query<T> implements TableQuery<T> {
  constructor(
    protected readonly runner: Runner,
    protected readonly name: string,
    private readonly state: QueryState = { where: [], order: [] }
  ) {}

  where(filter: Filter<T> | SqlFragment): TableQuery<T> {
    return new Query(this.runner, this.name, { ...this.state, where: [...this.state.where, ...conditions(filter)] });
  }

  orderBy(...order: OrderBy<T>[]): TableQuery<T> {
    return new Query(this.runner, this.name, { ...this.state, order: [...this.state.order, ...order.map(orderTerm)] });
  }

  limit(count: number): TableQuery<T> {
    return new Query(this.runner, this.name, { ...this.state, limit: count });
  }

  all(): Promise<T[]> {
    const { where, order, limit } = this.state;
    const parts = [new Fragment(`SELECT * FROM ${quote(this.name)}`)];
    if (where.length) parts.push(new Fragment("WHERE"), join(where, " AND "));
    if (order.length) parts.push(new Fragment("ORDER BY"), join(order, ", "));
    if (limit !== undefined) parts.push(new Fragment("LIMIT ?", [limit]));
    const statement = join(parts, " ");
    return this.runner.query<T>(statement.sql, [...statement.params]);
  }

  async first(): Promise<T | undefined> {
    const [row] = await this.limit(1).all();
    return row;
  }
}

export class Table<T> extends Query<T> implements StorageTable<T> {
  find(id: Id<T>): Promise<T | undefined> {
    return this.where({ id } as Filter<T>).first();
  }

  async insert(values: Values<T>): Promise<T> {
    const columns = assignments(values);
    const statement = columns.length
      ? join(
          [
            new Fragment(`INSERT INTO ${quote(this.name)} (${columns.map(([column]) => column).join(", ")}) VALUES (`),
            join(columns.map(([, value]) => value), ", "),
            new Fragment(") RETURNING *")
          ],
          ""
        )
      : new Fragment(`INSERT INTO ${quote(this.name)} DEFAULT VALUES RETURNING *`);
    const [row] = await this.runner.query<T>(statement.sql, [...statement.params]);
    return row;
  }

  update(target: Target<T>, values: Values<T>): Promise<T[]> {
    const where = this.target(target, "update");
    const columns = assignments(values);
    if (!columns.length) return this.where(where).all();
    const statement = join(
      [
        new Fragment(`UPDATE ${quote(this.name)} SET`),
        join(columns.map(([column, value]) => join([new Fragment(`${column} =`), value], " ")), ", "),
        new Fragment("WHERE"),
        where,
        new Fragment("RETURNING *")
      ],
      " "
    );
    return this.runner.query<T>(statement.sql, [...statement.params]);
  }

  async delete(target: Target<T>): Promise<number> {
    const where = this.target(target, "delete");
    const result = await this.runner.execute(`DELETE FROM ${quote(this.name)} WHERE ${where.sql}`, [...where.params]);
    return result.rowsAffected;
  }

  /** The WHERE condition for an update or delete, refusing one that would match every row. */
  private target(target: Target<T>, action: string): Fragment {
    const where = conditions(typeof target === "object" && target !== null ? target : { id: target });
    if (!where.length) {
      throw chainError(
        "INVALID_ARGUMENT",
        `${this.name}.${action}() was given no condition, so it would ${action} every row — pass sql\`1\` if that's intended`
      );
    }
    return join(where, " AND ");
  }
}
