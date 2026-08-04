"use strict";
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
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const path = __importStar(require("path"));
const fs = __importStar(require("fs"));
const cp = __importStar(require("child_process"));
const vscode_1 = require("vscode");
const node_1 = require("vscode-languageclient/node");
const dateTimeEditor_1 = require("./dateTimeEditor");
const blobPreview_1 = require("./blobPreview");
let client;
// ── Activate ──────────────────────────────────────────────────────────────────
function activate(context) {
    (0, dateTimeEditor_1.registerDateTimeEditor)(context);
    (0, blobPreview_1.registerBlobPreview)(context);
    const serverPath = resolveServerPath(context);
    if (!serverPath) {
        vscode_1.window
            .showErrorMessage("mdix-lsp binary not found. Build with `cargo build -p mdix-lsp` or set dixscript.server.path.", "Open Settings")
            .then(choice => {
            if (choice === "Open Settings") {
                vscode_1.commands.executeCommand("workbench.action.openSettings", "dixscript.server.path");
            }
        });
        return;
    }
    const cfg = vscode_1.workspace.getConfiguration("dixscript.server");
    const traceStr = cfg.get("trace", "off");
    const extraArgs = cfg.get("extraArgs", []);
    const serverOptions = {
        run: {
            command: serverPath,
            args: extraArgs,
            transport: node_1.TransportKind.stdio,
        },
        debug: {
            command: serverPath,
            args: ["--log-level", "debug", ...extraArgs],
            transport: node_1.TransportKind.stdio,
        },
    };
    const clientOptions = {
        documentSelector: [{ scheme: "file", language: "mdix" }],
        synchronize: {
            fileEvents: vscode_1.workspace.createFileSystemWatcher("**/*.mdix"),
        },
        traceOutputChannel: traceStr !== "off"
            ? vscode_1.window.createOutputChannel("DixScript LSP Trace")
            : undefined,
    };
    client = new node_1.LanguageClient("mdix-lsp", "DixScript Language Server", serverOptions, clientOptions);
    client.start();
    context.subscriptions.push(client);
    // Restart command — useful after rebuilding the binary during development.
    context.subscriptions.push(vscode_1.commands.registerCommand("dixscript.restartServer", async () => {
        if (client) {
            await client.stop();
            client.start();
            vscode_1.window.showInformationMessage("DixScript language server restarted.");
        }
    }));
}
// ── Deactivate ────────────────────────────────────────────────────────────────
async function deactivate() {
    if (client) {
        await client.stop();
    }
}
// ── Binary resolution ─────────────────────────────────────────────────────────
function resolveServerPath(context) {
    const exe = process.platform === "win32" ? "mdix-lsp.exe" : "mdix-lsp";
    // 1. User-configured path
    const userPath = vscode_1.workspace.getConfiguration("dixscript.server").get("path", "").trim();
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
function platformDir() {
    const map = {
        "linux-x64": "linux-x64",
        "linux-arm64": "linux-arm64",
        "darwin-x64": "darwin-x64",
        "darwin-arm64": "darwin-arm64",
        "win32-x64": "win32-x64",
    };
    return map[`${process.platform}-${process.arch}`];
}
function which(name) {
    try {
        const cmd = process.platform === "win32" ? `where ${name}` : `which ${name}`;
        const result = cp.execSync(cmd, { encoding: "utf8" }).trim().split("\n")[0];
        return result && fs.existsSync(result) ? result : undefined;
    }
    catch {
        return undefined;
    }
}
//# sourceMappingURL=extension.js.map