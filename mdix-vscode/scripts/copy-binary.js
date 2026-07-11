#!/usr/bin/env node
/**
 * Copies the compiled mdix-lsp binary into bin/{platform}/ so it gets
 * bundled inside the .vsix when running `vsce package --target <platform>`.
 *
 * Usage: node scripts/copy-binary.js [--release|--debug]
 *   --release  force target/release
 *   --debug    force target/debug
 *   (no flag)  auto-detect: prefer target/release if it exists, else
 *              target/debug. See note below on why this changed from a
 *              hardcoded "debug by default".
 */
const fs   = require("fs");
const path = require("path");

const platform   = `${process.platform}-${process.arch}`;
const binaryName = process.platform === "win32" ? "mdix-lsp.exe" : "mdix-lsp";

// Extension lives inside DixScript-Rust/vscode-dixscript/
const workspaceRoot = path.resolve(__dirname, "..", "..");

// ROOT CAUSE OF THE CI FAILURE ("Binary not found: .../target/debug/mdix-lsp"):
// this script used to default to "debug" unless called with --release. That's
// fine when you invoke it directly, but `vsce package` automatically re-runs
// the npm "vscode:prepublish" script internally as part of packaging -- and
// that script (in package.json) calls this file with NO arguments. So even
// though the CI workflow built with `cargo build --release` beforehand, the
// automatic re-invocation during `vsce package` ignored that and looked for
// target/debug, which was never built. Passing --release into the workflow's
// own call didn't help, because that call isn't the one that mattered -- the
// one vsce triggers internally is. Auto-detecting (prefer release if it's
// there) fixes it regardless of which invocation actually runs, and matches
// what you'd want locally too: if you've only ever built --release, this
// should just work without you having to remember a flag.
function resolveProfile() {
  if (process.argv.includes("--release")) return "release";
  if (process.argv.includes("--debug"))   return "debug";

  const releasePath = path.join(workspaceRoot, "target", "release", binaryName);
  if (fs.existsSync(releasePath)) return "release";
  return "debug";
}

const profile = resolveProfile();
const src  = path.join(workspaceRoot, "target", profile, binaryName);
const dest = path.join(__dirname, "..", "bin", platform, binaryName);

if (!fs.existsSync(src)) {
  console.error(`Binary not found: ${src}`);
  console.error(`Run: cargo build -p mdix-lsp${profile === "release" ? " --release" : ""}`);
  process.exit(1);
}

fs.mkdirSync(path.dirname(dest), { recursive: true });
fs.copyFileSync(src, dest);

if (process.platform !== "win32") {
  fs.chmodSync(dest, 0o755);
}

console.log(`Copied (${profile}): ${src}\n              to: ${dest}`);
