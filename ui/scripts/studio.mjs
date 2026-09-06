// Start the real Rust development bridge, then the Svelte workspace.
// Requires Rust on PATH. No mock state, browser compositor, or remote service.
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { resolve, join } from "node:path";
import { createServer } from "vite";

const root = fileURLToPath(new URL("../../", import.meta.url));
const ui = join(root, "ui");
let child;
let vite;
let stopping = false;
async function stop(code = 0) {
  if (stopping) return;
  stopping = true;
  child?.kill("SIGTERM");
  await vite?.close();
  process.exit(code);
}
process.once("SIGINT", () => void stop());
process.once("SIGTERM", () => void stop());

try {
  console.log("Building the Bonaparte Rust renderer…");
  await new Promise((done, fail) => {
    child = spawn("cargo", ["build", "-p", "bonaparte-preview"], { cwd: root, stdio: "inherit" });
    child.once("error", () =>
      fail(new Error("Rust is required. Install rustup, then make sure cargo is on PATH.")),
    );
    child.once("exit", (code) =>
      code === 0 ? done() : fail(new Error(`Rust build failed (${code}).`)),
    );
  });
  const target = resolve(root, process.env.CARGO_TARGET_DIR ?? "target");
  const executable = join(
    target,
    "debug",
    `bonaparte-preview${process.platform === "win32" ? ".exe" : ""}`,
  );
  await new Promise((done, fail) => {
    child = spawn(executable, [], {
      cwd: root,
      env: process.env,
      stdio: ["ignore", "inherit", "pipe"],
    });
    child.once("error", fail);
    child.stderr.on("data", (data) => {
      process.stderr.write(data);
      if (data.toString().includes("Bonaparte Rust render service on")) done();
    });
    child.once("exit", (code) => fail(new Error(`Rust service stopped (${code}).`)));
  });
  child.on("exit", (code) => {
    if (!stopping) void stop(code ?? 1);
  });
  vite = await createServer({ root: ui, configFile: join(ui, "vite.config.ts") });
  await vite.listen();
  vite.printUrls();
} catch (error) {
  console.error(error.message ?? error);
  await stop(1);
}
