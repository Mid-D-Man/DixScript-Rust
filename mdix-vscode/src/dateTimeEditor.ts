/**
 * Inline date/timestamp picker for DixScript `Date` and `Timestamp` literals.
 *
 * Handled entirely client-side: the `mdix-lsp` CodeLens provider emits a
 * "📅 Edit" lens over every `Date`/`Timestamp` token whose command is
 * `mdix.editDateTime`, carrying the document URI, the token's LSP range, its
 * current literal text, and its kind ("date" | "timestamp") as arguments.
 *
 * This command is registered here — NOT declared in the server's
 * `executeCommandProvider` list — so VS Code resolves it locally and never
 * round-trips through `workspace/executeCommand` at all.
 */

import * as vscode from "vscode";

interface LspPosition {
  line: number;
  character: number;
}

interface LspRange {
  start: LspPosition;
  end: LspPosition;
}

type DateTimeKind = "date" | "timestamp";

export function registerDateTimeEditor(context: vscode.ExtensionContext): void {
  context.subscriptions.push(
    vscode.commands.registerCommand(
      "mdix.editDateTime",
      (uriStr: string, range: LspRange, currentValue: string, kind: DateTimeKind) => {
        openDateTimeEditor(uriStr, range, currentValue, kind);
      }
    )
  );
}

let activePanel: vscode.WebviewPanel | undefined;

function openDateTimeEditor(
  uriStr: string,
  range: LspRange,
  currentValue: string,
  kind: DateTimeKind
): void {
  if (activePanel) {
    activePanel.dispose();
  }

  const panel = vscode.window.createWebviewPanel(
    "mdixDateTimeEditor",
    kind === "date" ? "Edit Date" : "Edit Timestamp",
    { viewColumn: vscode.ViewColumn.Beside, preserveFocus: false },
    { enableScripts: true, retainContextWhenHidden: false }
  );
  activePanel = panel;

  panel.webview.html = buildHtml(kind, currentValue);

  panel.webview.onDidReceiveMessage(async (message: { type: string; value?: string }) => {
    if (message.type === "apply" && message.value) {
      await applyEdit(uriStr, range, message.value);
      panel.dispose();
    } else if (message.type === "cancel") {
      panel.dispose();
    }
  });

  panel.onDidDispose(() => {
    if (activePanel === panel) {
      activePanel = undefined;
    }
  });
}

function toVscodeRange(r: LspRange): vscode.Range {
  return new vscode.Range(
    new vscode.Position(r.start.line, r.start.character),
    new vscode.Position(r.end.line, r.end.character)
  );
}

async function applyEdit(uriStr: string, range: LspRange, newValue: string): Promise<void> {
  const uri = vscode.Uri.parse(uriStr);

  let doc = vscode.workspace.textDocuments.find(d => d.uri.toString() === uri.toString());
  if (!doc) {
    try {
      doc = await vscode.workspace.openTextDocument(uri);
    } catch {
      vscode.window.showErrorMessage(`DixScript: could not open ${uriStr} to apply the edit.`);
      return;
    }
  }

  const edit = new vscode.WorkspaceEdit();
  edit.replace(uri, toVscodeRange(range), newValue);

  const applied = await vscode.workspace.applyEdit(edit);
  if (!applied) {
    vscode.window.showErrorMessage("DixScript: failed to apply the date/time edit.");
  }
}

const ZONE_SUFFIX = /(Z|[+-]\d{2}:\d{2})$/;

function buildHtml(kind: DateTimeKind, currentValue: string): string {
  const inputType = kind === "date" ? "date" : "datetime-local";
  const hasZone = kind === "timestamp" && ZONE_SUFFIX.test(currentValue);

  // <input type="datetime-local"> cannot represent a UTC/offset suffix at
  // all — strip it for display. The apply handler pads to HH:MM:SS if the
  // browser's step granularity ever omits seconds, matching the DixScript
  // Timestamp grammar (`YYYY-MM-DDThh:mm:ss[.fff][Z|±hh:mm]`), but does not
  // re-add whatever zone was stripped; the warning below makes that explicit.
  const inputValue = kind === "date" ? currentValue : currentValue.replace(ZONE_SUFFIX, "");

  const zoneWarning = hasZone
    ? `<div class="warn">⚠ Original had a zone/offset suffix — applying here removes it (local, unzoned result).</div>`
    : "";

  return `<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8">
<style>
  body {
    font-family: var(--vscode-font-family);
    color: var(--vscode-foreground);
    background: var(--vscode-editor-background);
    padding: 22px;
  }
  .row { margin-bottom: 14px; }
  input {
    font-size: 15px;
    padding: 6px 8px;
    background: var(--vscode-input-background);
    color: var(--vscode-input-foreground);
    border: 1px solid var(--vscode-input-border, transparent);
    border-radius: 4px;
  }
  .hint {
    opacity: 0.7;
    font-size: 12px;
    margin-top: 6px;
  }
  .warn {
    color: var(--vscode-editorWarning-foreground, #cca700);
    font-size: 12px;
    margin-top: 6px;
  }
  button {
    font-family: var(--vscode-font-family);
    font-size: 13px;
    padding: 6px 14px;
    border-radius: 4px;
    border: none;
    cursor: pointer;
    margin-right: 8px;
  }
  #apply {
    background: var(--vscode-button-background);
    color: var(--vscode-button-foreground);
  }
  #apply:hover { background: var(--vscode-button-hoverBackground); }
  #cancel {
    background: transparent;
    color: var(--vscode-foreground);
    border: 1px solid var(--vscode-button-secondaryBackground, #555);
  }
</style>
</head>
<body>
  <div class="row">
    <input id="value" type="${inputType}" step="1" value="${escapeAttr(inputValue)}" autofocus />
    <div class="hint">Current literal: <code>${escapeHtml(currentValue)}</code></div>
    ${zoneWarning}
  </div>
  <div class="row">
    <button id="apply">Apply</button>
    <button id="cancel">Cancel</button>
  </div>
  <script>
    const vscodeApi = acquireVsCodeApi();
    const input = document.getElementById("value");

    document.getElementById("apply").addEventListener("click", () => {
      let v = input.value;
      if (!v) { return; }
      if ("${kind}" === "timestamp" && v.length === 16) {
        v += ":00"; // pad missing seconds (HH:MM -> HH:MM:SS)
      }
      vscodeApi.postMessage({ type: "apply", value: v });
    });

    document.getElementById("cancel").addEventListener("click", () => {
      vscodeApi.postMessage({ type: "cancel" });
    });

    input.addEventListener("keydown", (e) => {
      if (e.key === "Enter") { document.getElementById("apply").click(); }
      if (e.key === "Escape") { document.getElementById("cancel").click(); }
    });
  </script>
</body>
</html>`;
}

function escapeAttr(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/"/g, "&quot;");
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
