#!/usr/bin/env node
/**
 * Copies compiled binaries into bin/{platform}/ so they get bundled inside
 * the .vsix when running `vsce package --target <platform>`.
 *
 * Usage: node scripts/copy-binary.js [--release|--debug]
 *   --release  force target/release
 *   --debug    force target/debug
 *   (no flag)  auto-detect: prefer target/release if it exists, else
 *              target/debug. See note below on why this changed from a
 *              hardcoded "debug by default".
 *
 * 2026-08-06 — now copies both mdix-lsp AND mdix (the mdix-cli crate's
 * binary — see mdix-cli/Cargo.toml's [[bin]] name = "mdix") into the same
 * bin/{platform}/ folder. This isn't just convenience: mdix-lsp's own CLI
 * resolution (which_mdix() in mdix-lsp/src/features/commands.rs) checks
 * PATH first, then falls back to std::env::current_exe().with_file_name
 * ("mdix") -- i.e. "look next to whatever binary is currently running".
 * Since the extension launches the bundled bin/{platform}/mdix-lsp copy,
 * putting `mdix` in that same folder means that fallback finds it with no
 * PATH setup, no settings, nothing extra required at runtime.
 */
const fs   = require("fs");
const path = require("path");

const platform = `${process.platform}-${process.arch}`;
const isWin    = process.platform === "win32";

// name here is the *source* filename (as cargo produces it); destName is
// what it should be called once copied — same as source on every platform
// except Windows, where both need the .exe suffix added.
const BINARIES = [
  { crate: "mdix-lsp", name: "mdix-lsp" },
  { crate: "mdix-cli", name: "mdix" }, // crate name differs from bin name
];

// Extension lives inside DixScript-Rust/mdix-vscode/
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
function resolveProfile(binaryName) {
  if (process.argv.includes("--release")) return "release";
  if (process.argv.includes("--debug"))   return "debug";

  const releasePath = path.join(workspaceRoot, "target", "release", binaryName);
  if (fs.existsSync(releasePath)) return "release";
  return "debug";
}

let hadError = false;

for (const { crate, name } of BINARIES) {
  const srcName = isWin ? `${name}.exe` : name;
  const profile = resolveProfile(srcName);
  const src  = path.join(workspaceRoot, "target", profile, srcName);
  const dest = path.join(__dirname, "..", "bin", platform, srcName);

  if (!fs.existsSync(src)) {
    console.error(`Binary not found: ${src}`);
    console.error(`Run: cargo build -p ${crate}${profile === "release" ? " --release" : ""}`);
    hadError = true;
    continue;
  }

  fs.mkdirSync(path.dirname(dest), { recursive: true });
  fs.copyFileSync(src, dest);

  if (!isWin) {
    fs.chmodSync(dest, 0o755);
  }

  console.log(`Copied (${profile}): ${src}\n              to: ${dest}`);
}

if (hadError) process.exit(1);
