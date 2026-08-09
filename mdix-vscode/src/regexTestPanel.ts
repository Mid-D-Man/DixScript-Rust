/**
 * DixScript regex tester webview.
 *
 * Runs test input through mdix-lsp's `mdix.testRegex` command rather than
 * JS RegExp — DixScript's `regex` type is backed by Rust's `regex` crate
 * (v1.10), which deliberately doesn't support backreferences or lookaround
 * (linear-time guarantee). A JS-side tester could show matches DixScript
 * itself would never produce, so this always goes through the real server.
 */

import * as vscode from "vscode";
import { LanguageClient } from "vscode-languageclient/node";

interface RegexMatchGroup {
  name:  string | null;
  value: string | null;
}
interface RegexMatch {
  start:  number;
  end:    number;
  text:   string;
  groups: RegexMatchGroup[];
}
interface RegexTestResult {
  valid:   boolean;
  error:   string | null;
  matches: RegexMatch[];
}

let panel: vscode.WebviewPanel | undefined;

export function registerRegexTester(
  context: vscode.ExtensionContext,
  getClient: () => LanguageClient | undefined
): void {
  context.subscriptions.push(
    vscode.commands.registerCommand("dixscript.testRegex", () => {
      openPanel(context, getClient);
    })
  );
}

function openPanel(
  context: vscode.ExtensionContext,
  getClient: () => LanguageClient | undefined
): void {
  if (panel) {
    panel.reveal();
    return;
  }

  panel = vscode.window.createWebviewPanel(
    "dixscriptRegexTester",
    "DixScript Regex Tester",
    vscode.ViewColumn.Beside,
    { enableScripts: true, retainContextWhenHidden: true }
  );

  // Reusing the extension's existing file icons for now rather than adding
  // a new asset — swap these two paths for a dedicated regex icon later if
  // you want the tab visually distinct from a plain .mdix file tab.
  panel.iconPath = {
    light: vscode.Uri.joinPath(context.extensionUri, "icons", "mdix-file-light.svg"),
    dark:  vscode.Uri.joinPath(context.extensionUri, "icons", "mdix-file-dark.svg"),
  };

  panel.webview.html = getHtml();
  panel.onDidDispose(() => { panel = undefined; }, null, context.subscriptions);

  panel.webview.onDidReceiveMessage(
    async (msg: { type: string; pattern: string; testText: string }) => {
      if (msg.type !== "test") return;

      const client = getClient();
      if (!client) {
        panel?.webview.postMessage({
          type: "result",
          result: { valid: false, error: "Language server not running.", matches: [] },
        });
        return;
      }

      try {
        const result = await client.sendRequest<RegexTestResult>("workspace/executeCommand", {
          command:   "mdix.testRegex",
          arguments: [msg.pattern, msg.testText],
        });
        panel?.webview.postMessage({ type: "result", result });
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        panel?.webview.postMessage({
          type: "result",
          result: { valid: false, error: message, matches: [] },
        });
      }
    },
    undefined,
    context.subscriptions
  );
}

function getHtml(): string {
  return /* html */ `<!DOCTYPE html>
<html>
<head>
<style>
  body { font-family: var(--vscode-font-family); padding: 12px; color: var(--vscode-foreground); }
  label { display: block; margin-top: 12px; margin-bottom: 4px; font-weight: 600; }
  input, textarea {
    width: 100%; box-sizing: border-box; background: var(--vscode-input-background);
    color: var(--vscode-input-foreground); border: 1px solid var(--vscode-input-border);
    padding: 6px; font-family: var(--vscode-editor-font-family); font-size: 13px;
  }
  textarea { height: 120px; resize: vertical; }
  #result { margin-top: 12px; white-space: pre-wrap; font-family: var(--vscode-editor-font-family); line-height: 1.6; }
  mark { background: var(--vscode-editor-findMatchHighlightBackground); color: inherit; border-radius: 2px; }
  .error { color: var(--vscode-errorForeground); }
  .status { margin-top: 8px; font-size: 12px; opacity: 0.8; }
  .group { margin-left: 16px; opacity: 0.85; font-size: 12px; }
  hr { border: none; border-top: 1px solid var(--vscode-input-border); margin: 10px 0; }
</style>
</head>
<body>
  <label for="pattern">Pattern</label>
  <input id="pattern" placeholder="e.g. ^[A-Z][a-z]+$" />

  <label for="testText">Test text</label>
  <textarea id="testText" placeholder="Paste text to test against the pattern..."></textarea>

  <div id="status" class="status"></div>
  <div id="result"></div>

<script>
  const vscode = acquireVsCodeApi();
  const patternEl  = document.getElementById('pattern');
  const testTextEl = document.getElementById('testText');
  const statusEl   = document.getElementById('status');
  const resultEl   = document.getElementById('result');

  let debounceTimer;
  function scheduleRun() {
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(run, 200);
  }

  function run() {
    const pattern  = patternEl.value;
    const testText = testTextEl.value;
    if (!pattern) { resultEl.textContent = ''; statusEl.textContent = ''; return; }
    vscode.postMessage({ type: 'test', pattern, testText });
  }

  patternEl.addEventListener('input', scheduleRun);
  testTextEl.addEventListener('input', scheduleRun);

  window.addEventListener('message', (event) => {
    const msg = event.data;
    if (msg.type !== 'result') return;
    render(msg.result);
  });

  function escapeHtml(s) {
    return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  }

  function render(result) {
    if (!result.valid) {
      statusEl.innerHTML = '<span class="error">Invalid pattern: ' + escapeHtml(result.error || 'unknown error') + '</span>';
      resultEl.textContent = '';
      return;
    }

    const testText = testTextEl.value;
    statusEl.textContent = result.matches.length + ' match' + (result.matches.length === 1 ? '' : 'es');

    let html = '';
    let last = 0;
    for (const m of result.matches) {
      html += escapeHtml(testText.slice(last, m.start));
      html += '<mark>' + escapeHtml(testText.slice(m.start, m.end)) + '</mark>';
      last = m.end;
    }
    html += escapeHtml(testText.slice(last));

    let groupsHtml = '';
    result.matches.forEach((m, i) => {
      if (m.groups.some(g => g.value !== null)) {
        groupsHtml += '<div class="group">match ' + (i + 1) + ': ';
        groupsHtml += m.groups.map((g, gi) =>
          (g.name || ('#' + (gi + 1))) + '=' + (g.value !== null ? JSON.stringify(g.value) : 'null')
        ).join('  ');
        groupsHtml += '</div>';
      }
    });

    resultEl.innerHTML = html + (groupsHtml ? '<hr/>' + groupsHtml : '');
  }
</script>
</body>
</html>`;
}
