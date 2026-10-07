// Migration engine tests against real SQLite: every schema change is
// declared as EF-style classes, applied forward, compared with what the
// database actually ends up containing, then applied backward. Runs on
// the built output: `npm run build && npm test`.

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";

import { planMigration, noRenames } from "../dist/schema/diff.js";
import { introspect } from "../dist/schema/introspect.js";
import { canonicalize, comparable, modelsEqual } from "../dist/schema/model.js";
import { readSchemaClasses } from "../dist/schema/read-classes.js";

const header = 'import { Table, Column, PrimaryKey, ForeignKey, Index, Trigger, NotMapped } from "@chain/sdk/schema";\n';

function model(source) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "chain-schema-"));
  fs.writeFileSync(path.join(dir, "schema.ts"), header + source);
  try {
    return readSchemaClasses(dir);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

/** Runs migration SQL the way the runner does: legacy_alter_table on, foreign keys off. */
function run(db, sql) {
  db.exec("PRAGMA legacy_alter_table = ON; PRAGMA foreign_keys = OFF;");
  if (sql) db.exec(sql);
}

function assertSchema(db, expected) {
  const actual = introspect(db).model;
  if (!modelsEqual(actual, expected)) {
    assert.deepEqual(comparable(canonicalize(actual)), comparable(canonicalize(expected)));
  }
}

/** Builds `from`, seeds it, migrates up to `to` and back down, checking the schema both ways. */
function roundTrip(fromSource, toSource, { renames = noRenames(), seed = "", after = () => {} } = {}) {
  const from = model(fromSource);
  const to = model(toSource);
  const db = new DatabaseSync(":memory:");
  run(db, planMigration({ tables: [] }, from).up);
  if (seed) db.exec(seed);
  const plan = planMigration(from, to, renames);
  run(db, plan.up);
  assertSchema(db, to);
  after(db, plan);
  run(db, plan.down);
  assertSchema(db, from);
  return plan;
}

const notes = `
@Table()
export class Note {
  @PrimaryKey({ autoIncrement: true }) id!: number;
  title!: string;
  body!: string | null;
}
`;

test("reads EF-style classes: types, nullability, conventions", () => {
  const { tables } = model(`
@Table("course_item")
export class CourseItem {
  id!: number;
  name!: string;
  note?: string;
  done!: boolean;
  score!: number | null;
  @Column({ type: "real" }) weight!: number;
  data!: Uint8Array | null;
  @NotMapped() cached!: string;
}
`);
  const [table] = tables;
  assert.equal(table.name, "course_item");
  assert.deepEqual(table.primaryKey, { columns: ["id"], autoIncrement: false });
  assert.deepEqual(
    table.columns.map((c) => [c.name, c.type, c.notNull]),
    [
      ["id", "INTEGER", true],
      ["name", "TEXT", true],
      ["note", "TEXT", false],
      ["done", "INTEGER", true],
      ["score", "INTEGER", false],
      ["weight", "REAL", true],
      ["data", "BLOB", false]
    ]
  );
});

test("class name becomes a snake_case table name by default", () => {
  assert.equal(model(`@Table() export class AiAction { id!: number; }`).tables[0].name, "ai_action");
});

test("creates tables from nothing, and down drops them", () => {
  const plan = roundTrip("", notes);
  assert.match(plan.up, /CREATE TABLE "note"/);
  assert.match(plan.down, /DROP TABLE "note"/);
});

test("adds a nullable column in place", () => {
  const plan = roundTrip(notes, notes.replace("body!: string | null;", "body!: string | null;\n  pinned!: number | null;"), {
    seed: "INSERT INTO note (title) VALUES ('a');"
  });
  assert.match(plan.up, /ADD COLUMN "pinned" INTEGER/);
  assert.doesNotMatch(plan.up, /Rebuild/);
});

test("new NOT NULL column without default: rebuilds and backfills, like EF", () => {
  const plan = roundTrip(notes, notes.replace("title!: string;", "title!: string;\n  status!: number;"), {
    seed: "INSERT INTO note (title) VALUES ('a');",
    after: (db) => assert.equal(db.prepare("SELECT status FROM note").get().status, 0)
  });
  assert.match(plan.warnings.join(), /status has no default; existing rows get 0/);
});

test("expression default forces a rebuild and keeps the data", () => {
  roundTrip(notes, notes.replace("title!: string;", `title!: string;\n  @Column({ defaultSql: "datetime('now')" }) created_at!: string;`), {
    seed: "INSERT INTO note (title, body) VALUES ('keep me', 'b');",
    after: (db) => {
      const row = db.prepare("SELECT title, body, created_at FROM note").get();
      assert.deepEqual([row.title, row.body], ["keep me", "b"]);
      assert.match(row.created_at, /^\d{4}-/);
    }
  });
});

test("drops a column, and down restores it (empty)", () => {
  const plan = roundTrip(notes, notes.replace("  body!: string | null;\n", ""), {
    seed: "INSERT INTO note (title, body) VALUES ('t', 'b');"
  });
  assert.match(plan.warnings.join(), /Drops column note.body/);
});

test("renames a column in place, keeping its data", () => {
  const renames = { tables: new Map(), columns: new Map([["note", new Map([["content", "body"]])]]) };
  const plan = roundTrip(notes, notes.replace("body!", "content!"), {
    renames,
    seed: "INSERT INTO note (title, body) VALUES ('t', 'kept');",
    after: (db) => assert.equal(db.prepare("SELECT content FROM note").get().content, "kept")
  });
  assert.match(plan.up, /RENAME COLUMN "body" TO "content"/);
  assert.match(plan.down, /RENAME COLUMN "content" TO "body"/);
});

test("renames a table", () => {
  const renames = { tables: new Map([["note", "memo"]]), columns: new Map() };
  roundTrip(notes, notes.replace("@Table()", '@Table("memo")'), {
    renames,
    seed: "INSERT INTO note (title) VALUES ('t');",
    after: (db) => assert.equal(db.prepare("SELECT count(*) n FROM memo").get().n, 1)
  });
});

test("making a column NOT NULL coalesces existing NULLs", () => {
  roundTrip(notes, notes.replace("body!: string | null;", "body!: string;"), {
    seed: "INSERT INTO note (title) VALUES ('t');",
    after: (db) => assert.equal(db.prepare("SELECT body FROM note").get().body, "")
  });
});

test("indexes, unique, and partial indexes", () => {
  const plan = roundTrip(
    notes,
    `
@Table()
@Index({ name: "note_title_body", columns: ["title", "body"], unique: true })
export class Note {
  @PrimaryKey({ autoIncrement: true }) id!: number;
  @Index({ where: "body IS NOT NULL" }) title!: string;
  body!: string | null;
}
`
  );
  assert.match(plan.up, /CREATE UNIQUE INDEX "note_title_body"/);
  assert.match(plan.up, /CREATE INDEX "idx_note_title" ON "note" \("title"\) WHERE body IS NOT NULL/);
  assert.doesNotMatch(plan.up, /Rebuild/);
});

const withCourses = `
@Table()
export class Course {
  @PrimaryKey({ autoIncrement: true }) id!: number;
  name!: string;
}
@Table()
@Trigger("module_needs_course", \`CREATE TRIGGER module_needs_course BEFORE INSERT ON module BEGIN
  SELECT RAISE(ABORT, 'no course') WHERE NOT EXISTS (SELECT 1 FROM course WHERE id = NEW.course_id);
END\`)
export class Module {
  @PrimaryKey({ autoIncrement: true }) id!: number;
  @ForeignKey(() => Course, { onDelete: "cascade" }) course_id!: number;
  @Column({ check: "length(name) > 0" }) name!: string;
}
`;

test("foreign keys, checks, and triggers", () => {
  roundTrip(notes, notes + withCourses);
});

test("rebuilding a parent keeps child rows and other tables' triggers working", () => {
  roundTrip(withCourses, withCourses.replace("name!: string;\n}", "name!: string;\n  @Column({ defaultSql: \"datetime('now')\" }) created_at!: string;\n}"), {
    seed: "INSERT INTO course (name) VALUES ('c'); INSERT INTO module (course_id, name) VALUES (1, 'm');",
    after: (db) => {
      assert.equal(db.prepare("SELECT count(*) n FROM module").get().n, 1, "cascade must not fire during the rebuild");
      assert.throws(() => db.exec("INSERT INTO module (course_id, name) VALUES (99, 'x')"), /no course/);
    }
  });
});

test("changing a trigger drops and recreates only that trigger", () => {
  const plan = roundTrip(withCourses, withCourses.replace("'no course'", "'Pick a course first.'"));
  assert.match(plan.up, /DROP TRIGGER "module_needs_course"/);
  assert.doesNotMatch(plan.up, /Rebuild/);
});

test("no changes produces an empty plan", () => {
  assert.equal(planMigration(model(withCourses), model(withCourses)).empty, true);
});

test("unmappable types are reported with file and line", () => {
  assert.throws(() => model(`@Table() export class Bad { id!: number; when!: Date; }`), /schema\.ts:\d+: can't map the type of `bad.when`/);
});
