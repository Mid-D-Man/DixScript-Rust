"use strict";
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
const vscode_1 = require("vscode");
const node_1 = require("vscode-languageclient/node");
let client;
function activate(context) {
    const serverPath = resolveServerPath(context);
    if (!serverPath) {
        vscode_1.window.showErrorMessage("mdix-lsp binary not found. " +
            "Run `cargo build` from the DixScript-Rust workspace root, " +
            "or set the MDIX_LSP_PATH environment variable.");
        return;
    }
    const serverOptions = {
        run: { command: serverPath, transport: node_1.TransportKind.stdio },
        debug: { command: serverPath, transport: node_1.TransportKind.stdio },
    };
    const clientOptions = {
        // Activate for all .mdix files.
        documentSelector: [{ scheme: "file", language: "mdix" }],
        synchronize: {
            // Watch for .mdix changes on disk so the server can invalidate caches.
            fileEvents: require("vscode").workspace.createFileSystemWatcher("**/*.mdix"),
        },
    };
    client = new node_1.LanguageClient("mdix-lsp", "DixScript Language Server", serverOptions, clientOptions);
    client.start();
    context.subscriptions.push(client);
    vscode_1.window.showInformationMessage(`mdix-lsp started from: ${serverPath}`);
}
async function deactivate() {
    if (client) {
        await client.stop();
    }
}
// ── Binary path resolution ─────────────────────────────────────────────────────
function resolveServerPath(context) {
    // 1. Explicit override via environment variable.
    const envPath = process.env["MDIX_LSP_PATH"];
    if (envPath && fs.existsSync(envPath)) {
        return envPath;
    }
    // 2. Workspace cargo target (debug build — fastest to iterate on).
    //    The extension lives inside DixScript-Rust/vscode-mdix-test/,
    //    so two directories up is the workspace root.
    const workspaceRoot = path.resolve(context.extensionPath, "..");
    const binaryName = process.platform === "win32" ? "mdix-lsp.exe" : "mdix-lsp";
    const candidates = [
        path.join(workspaceRoot, "target", "debug", binaryName),
        path.join(workspaceRoot, "target", "release", binaryName),
    ];
    for (const candidate of candidates) {
        if (fs.existsSync(candidate)) {
            return candidate;
        }
    }
    return undefined;
}
//# sourceMappingURL=extension.js.map