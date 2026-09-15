import { execFileSync, execSync } from "node:child_process";
import readline from "node:readline/promises";

function hasCommand(cmd: string): boolean {
  try {
    execFileSync(cmd, ["--version"], { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

async function confirm(question: string): Promise<boolean> {
  if (!process.stdin.isTTY) return false;
  const rl = readline.createInterface({ input: process.stdin, output: process.stdout });
  try {
    const answer = await rl.question(`${question} [y/N] `);
    return /^y(es)?$/i.test(answer.trim());
  } finally {
    rl.close();
  }
}

/**
 * Checks for the Rust toolchain, which chain-core (crates/core) needs to
 * build. Never installs anything without an explicit y/N confirmation —
 * this modifies the developer's machine, not just repo files.
 */
export async function doctor(): Promise<boolean> {
  console.log("Checking Chain development prerequisites...\n");

  const hasCargo = hasCommand("cargo");
  console.log(
    `  ${hasCargo ? "✔" : "✘"} Rust toolchain (cargo)${
      hasCargo ? "" : " — required to build the native (Tauri) side of a Chain app"
    }`
  );

  if (hasCargo) {
    console.log("\nAll set.");
    return true;
  }

  const proceed = await confirm(
    "\nInstall the Rust toolchain now via the official rustup.rs installer?"
  );
  if (!proceed) {
    console.log("\nSkipped. Install it yourself when ready: https://rustup.rs");
    return false;
  }

  console.log("\nInstalling Rust via rustup...");
  execSync("curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y", {
    stdio: "inherit"
  });
  console.log(
    '\nRust installed. Open a new shell (or run `source "$HOME/.cargo/env"`) so `cargo` is on PATH.'
  );
  return true;
}

export function checkRustQuietly(): boolean {
  return hasCommand("cargo");
}
