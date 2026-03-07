/**
 * DixScript LSP test harness for VS Code.
 *
 * Resolves the mdix-lsp binary from the workspace's cargo target directory
 * and launches it as a stdio language server. This is a dev-only extension
 * and is never published to the VS Code marketplace.
 *
 * Setup:
 *   1. cargo build  (from DixScript-Rust/ workspace root)
 *   2. cd vscode-mdix-test && npm install && npm run compile
 *   3. Open vscode-mdix-test/ in VS Code and press F5 to launch the
 *      Extension Development Host with a .mdix file open.
 *
 * The MDIX_LSP_PATH environment variable overrides the default binary path,
 * which is useful when testing a release build or a custom install location.
 */

import * as path from "path";
import * as fs from "fs";
import { ExtensionContext, window } from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export function activate(context: ExtensionContext): void {
  const serverPath = resolveServerPath(context);

  if (!serverPath) {
    window.showErrorMessage(
      "mdix-lsp binary not found. " +
        "Run `cargo build` from the DixScript-Rust workspace root, " +
        "or set the MDIX_LSP_PATH environment variable."
    );
    return;
  }

  const serverOptions: ServerOptions = {
    run:   { command: serverPath, transport: TransportKind.stdio },
    debug: { command: serverPath, transport: TransportKind.stdio },
  };

  const clientOptions: LanguageClientOptions = {
    // Activate for all .mdix files.
    documentSelector: [{ scheme: "file", language: "mdix" }],
    synchronize: {
      // Watch for .mdix changes on disk so the server can invalidate caches.
      fileEvents: require("vscode").workspace.createFileSystemWatcher("**/*.mdix"),
    },
  };

  client = new LanguageClient(
    "mdix-lsp",
    "DixScript Language Server",
    serverOptions,
    clientOptions
  );

  client.start();
  context.subscriptions.push(client);

  window.showInformationMessage(`mdix-lsp started from: ${serverPath}`);
}

export async function deactivate(): Promise<void> {
  if (client) {
    await client.stop();
  }
}

// ── Binary path resolution ─────────────────────────────────────────────────────

function resolveServerPath(context: ExtensionContext): string | undefined {
  // 1. Explicit override via environment variable.
  const envPath = process.env["MDIX_LSP_PATH"];
  if (envPath && fs.existsSync(envPath)) {
    return envPath;
  }

  // 2. Workspace cargo target (debug build — fastest to iterate on).
  //    The extension lives inside DixScript-Rust/vscode-mdix-test/,
  //    so two directories up is the workspace root.
  const workspaceRoot = path.resolve(context.extensionPath, "..");
  const binaryName    = process.platform === "win32" ? "mdix-lsp.exe" : "mdix-lsp";

  const candidates = [
    path.join(workspaceRoot, "target", "debug",   binaryName),
    path.join(workspaceRoot, "target", "release",  binaryName),
  ];

  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }

  return undefined;
}
