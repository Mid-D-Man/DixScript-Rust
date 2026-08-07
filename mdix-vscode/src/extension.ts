/**
 * DixScript VS Code extension.
 *
 * Starts mdix-lsp as a stdio language server. Binary resolution order:
 *   1. User setting  dixscript.server.path
 *   2. MDIX_LSP_PATH env var  (useful in CI / dev)
 *   3. Bundled binary  bin/{platform}/mdix-lsp[.exe]
 *   4. Cargo target    ../target/{debug,release}/mdix-lsp[.exe]
 *   5. System PATH
 */

import * as path      from "path";
import * as fs        from "fs";
import * as cp        from "child_process";
import { ExtensionContext, window, workspace, commands } from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from "vscode-languageclient/node";

import { registerDateTimeEditor } from "./dateTimeEditor";
import { registerBlobPreview }    from "./blobPreview";
import { registerThemeColors }    from "./themeColors";
import { registerSettingsSync }   from "./settingsSync";

let client: LanguageClient | undefined;

// ── Activate ──────────────────────────────────────────────────────────────────

export function activate(context: ExtensionContext): void {
  registerDateTimeEditor(context);
  registerBlobPreview(context);
  // Getter, not `client` itself — `client` isn't assigned until further down
  // this function. The closure re-reads it lazily whenever the command
  // actually runs (module-scope `let`, captured by reference), so it always
  // sees the real client once it exists.
  registerThemeColors(context, () => client);
  registerSettingsSync(context, () => client);

  const serverPath = resolveServerPath(context);

  if (!serverPath) {
    window
      .showErrorMessage(
        "mdix-lsp binary not found. Build with `cargo build -p mdix-lsp` or set dixscript.server.path.",
        "Open Settings"
      )
      .then(choice => {
        if (choice === "Open Settings") {
          commands.executeCommand("workbench.action.openSettings", "dixscript.server.path");
        }
      });
    return;
  }

  const cfg       = workspace.getConfiguration("dixscript.server");
  const traceStr  = cfg.get<string>("trace", "off");
  const extraArgs = cfg.get<string[]>("extraArgs", []);

  const serverOptions: ServerOptions = {
    run: {
      command:   serverPath,
      args:      extraArgs,
      transport: TransportKind.stdio,
    },
    debug: {
      command:   serverPath,
      args:      ["--log-level", "debug", ...extraArgs],
      transport: TransportKind.stdio,
    },
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "mdix" }],
    synchronize: {
      fileEvents: workspace.createFileSystemWatcher("**/*.mdix"),
    },
    traceOutputChannel: traceStr !== "off"
      ? window.createOutputChannel("DixScript LSP Trace")
      : undefined,
  };

  client = new LanguageClient(
    "mdix-lsp",
    "DixScript Language Server",
    serverOptions,
    clientOptions
  );

  client.start();
  context.subscriptions.push(client);

  // Restart command — useful after rebuilding the binary during development.
  context.subscriptions.push(
    commands.registerCommand("dixscript.restartServer", async () => {
      if (client) {
        await client.stop();
        client.start();
        window.showInformationMessage("DixScript language server restarted.");
      }
    })
  );
}

// ── Deactivate ────────────────────────────────────────────────────────────────

export async function deactivate(): Promise<void> {
  if (client) {
    await client.stop();
  }
}

// ── Binary resolution ─────────────────────────────────────────────────────────

function resolveServerPath(context: ExtensionContext): string | undefined {
  const exe = process.platform === "win32" ? "mdix-lsp.exe" : "mdix-lsp";

  // 1. User-configured path
  const userPath = workspace.getConfiguration("dixscript.server").get<string>("path", "").trim();
  if (userPath && fs.existsSync(userPath)) {
    return userPath;
  }

  // 2. Environment variable (dev / CI override)
  const envPath = process.env["MDIX_LSP_PATH"];
  if (envPath && fs.existsSync(envPath)) {
    return envPath;
  }

  // 3. Bundled binary shipped inside the extension VSIX
  const platformKey = platformDir();
  if (platformKey) {
    const bundled = path.join(context.extensionPath, "bin", platformKey, exe);
    if (fs.existsSync(bundled)) {
      return bundled;
    }
  }

  // 4. Cargo build output (development — extension lives inside the workspace)
  const wsRoot = path.resolve(context.extensionPath, "..");
  for (const profile of ["debug", "release"]) {
    const candidate = path.join(wsRoot, "target", profile, exe);
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }

  // 5. System PATH
  return which(exe);
}

function platformDir(): string | undefined {
  const map: Record<string, string> = {
    "linux-x64":    "linux-x64",
    "linux-arm64":  "linux-arm64",
    "darwin-x64":   "darwin-x64",
    "darwin-arm64": "darwin-arm64",
    "win32-x64":    "win32-x64",
  };
  return map[`${process.platform}-${process.arch}`];
}

function which(name: string): string | undefined {
  try {
    const cmd    = process.platform === "win32" ? `where ${name}` : `which ${name}`;
    const result = cp.execSync(cmd, { encoding: "utf8" }).trim().split("\n")[0];
    return result && fs.existsSync(result) ? result : undefined;
  } catch {
    return undefined;
  }
          }
