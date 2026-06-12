#!/usr/bin/env node
/**
 * Copies the compiled mdix-lsp binary into bin/{platform}/ so it gets
 * bundled inside the .vsix when running `vsce package --target <platform>`.
 *
 * Usage: node scripts/copy-binary.js [--release]
 *   --release  copies from target/release (default: target/debug)
 */
const fs   = require("fs");
const path = require("path");

const profile    = process.argv.includes("--release") ? "release" : "debug";
const platform   = `${process.platform}-${process.arch}`;
const binaryName = process.platform === "win32" ? "mdix-lsp.exe" : "mdix-lsp";

// Extension lives inside DixScript-Rust/vscode-dixscript/
const workspaceRoot = path.resolve(__dirname, "..", "..");
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

console.log(`Copied: ${src}\n    to: ${dest}`);
