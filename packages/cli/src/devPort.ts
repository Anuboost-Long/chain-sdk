import fs from "node:fs";
import net from "node:net";
import path from "node:path";

import { nativeProjectDir } from "./nativeProject.js";

/** The template's Vite port (vite.config.ts) and tauri.conf.json's devUrl. */
export const DEFAULT_DEV_PORT = 1420;

/** Whether `port` can be listened on at `host`. A host this machine
 * doesn't have (no IPv6) can't clash, so it counts as free. */
function canListen(port: number, host: string): Promise<boolean> {
  return new Promise((resolve) => {
    const server = net.createServer();
    server.once("error", (error: NodeJS.ErrnoException) => resolve(error.code === "EADDRNOTAVAIL"));
    server.listen({ port, host, exclusive: true }, () => server.close(() => resolve(true)));
  });
}

/** The first port from `start` up that's free on both loopbacks — Vite
 * listens on `localhost`, which can be either one. */
export async function freeDevPort(start = DEFAULT_DEV_PORT): Promise<number> {
  for (let port = start; port < start + 100; port++) {
    if ((await canListen(port, "127.0.0.1")) && (await canListen(port, "::1"))) return port;
  }
  throw new Error(`no free port between ${start} and ${start + 99} for the dev server`);
}

/** `tauri dev --config` JSON pointing the window and the dev server at
 * `port`: devUrl, and `--port` appended to an `npm run` beforeDevCommand
 * (a command-line port beats vite.config.ts's). Null when the port is the
 * default, or when beforeDevCommand isn't an `npm run` script, so there's
 * no way to hand it the port. */
export function devPortConfig(beforeDevCommand: unknown, port: number): string | null {
  if (port === DEFAULT_DEV_PORT) return null;
  const script =
    typeof beforeDevCommand === "object" && beforeDevCommand !== null
      ? (beforeDevCommand as { script?: unknown }).script
      : beforeDevCommand;
  if (typeof script !== "string" || !/^npm run \S+$/.test(script.trim())) return null;
  const withPort = `${script.trim()} -- --port ${port}`;
  return JSON.stringify({
    build: {
      devUrl: `http://localhost:${port}`,
      beforeDevCommand:
        typeof beforeDevCommand === "string" ? withPort : { ...(beforeDevCommand as object), script: withPort }
    }
  });
}

export function readBeforeDevCommand(cwd: string): unknown {
  const conf = JSON.parse(fs.readFileSync(path.join(nativeProjectDir(cwd), "tauri.conf.json"), "utf8"));
  return conf.build?.beforeDevCommand;
}
