// chain dev's port choice when 1420 is taken (lazify request 13). Runs on
// the built output: `npm run build && npm test`.

import assert from "node:assert/strict";
import net from "node:net";
import { test } from "node:test";

import { devPortConfig, freeDevPort } from "../dist/devPort.js";

function listen(port, host) {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once("error", reject);
    server.listen({ port, host, exclusive: true }, () => resolve(server));
  });
}

async function someFreePort() {
  const server = await listen(0, "127.0.0.1");
  const { port } = server.address();
  await new Promise((resolve) => server.close(resolve));
  return port;
}

for (const host of ["127.0.0.1", "::1"]) {
  test(`skips a port held on ${host}`, async () => {
    const start = await someFreePort();
    const held = await listen(start, host);
    try {
      const port = await freeDevPort(start);
      assert.ok(port > start, `${port}`);
    } finally {
      held.close();
    }
  });
}

test("a free start port is used as is", async () => {
  const start = await someFreePort();
  assert.equal(await freeDevPort(start), start);
});

test("the default port needs no config", () => {
  assert.equal(devPortConfig("npm run dev:web", 1420), null);
});

test("another port moves devUrl and the npm script", () => {
  assert.deepEqual(JSON.parse(devPortConfig("npm run dev:web", 1421)), {
    build: { devUrl: "http://localhost:1421", beforeDevCommand: "npm run dev:web -- --port 1421" }
  });
  assert.deepEqual(JSON.parse(devPortConfig({ script: "npm run dev:web", wait: false }, 1422)).build.beforeDevCommand, {
    script: "npm run dev:web -- --port 1422",
    wait: false
  });
});

test("a command chain dev can't pass a port to gives no config", () => {
  for (const command of ["vite", "npm run dev:web -- --host", undefined, { cwd: "." }]) {
    assert.equal(devPortConfig(command, 1421), null, JSON.stringify(command));
  }
});
