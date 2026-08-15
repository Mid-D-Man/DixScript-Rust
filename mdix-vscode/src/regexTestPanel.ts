/**
 * DixScript regex tester webview.
 *
 * Runs test input through mdix-lsp's `mdix.testRegex` command rather than
 * JS RegExp — DixScript's `regex` type is backed by Rust's `regex` crate
 * (v1.10), which deliberately doesn't support backreferences or lookaround
 * (linear-time guarantee). A JS-side tester could show matches DixScript
 * itself would never produce, so this always goes through the real server.
 *
 * Two ways in:
 *   - `dixscript.testRegex` (command palette) — opens empty.
 *   - `mdix.previewRegex` (mdix-lsp's "🔍 Test Regex" CodeLens, same
 *     mechanism as `mdix.previewBlob`) — opens pre-filled with the clicked
 *     `r:(...)` literal's pattern. Client-only, not in ALL_COMMANDS, same
 *     reasoning as blobPreview.ts/dateTimeEditor.ts: vscode-languageclient
 *     checks for a locally-registered command with this ID before ever
 *     forwarding the CodeLens click to the server.
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

  // Positional args match the CodeLens's `arguments` array order exactly —
  // same convention as mdix.previewBlob/mdix.editDateTime.
  context.subscriptions.push(
    vscode.commands.registerCommand(
      "mdix.previewRegex",
      (_uriStr: string, pattern: string) => {
        openPanel(context, getClient, pattern);
      }
    )
  );
}

function openPanel(
  context: vscode.ExtensionContext,
  getClient: () => LanguageClient | undefined,
  initialPattern?: string
): void {
  if (panel) {
    panel.reveal();
    if (initialPattern !== undefined) {
      // Already open — push the new pattern in via postMessage instead of
      // regenerating the HTML (would lose whatever test text is already
      // typed in).
      panel.webview.postMessage({ type: "setPattern", pattern: initialPattern });
    }
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

  // Baked into the initial HTML rather than sent via a follow-up
  // postMessage: postMessage right after setting .html races the webview's
  // own script actually finishing evaluation and attaching its listener on
  // first load. Embedding it in the generated markup has no such race.
  panel.webview.html = getHtml(initialPattern);
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

function getHtml(initialPattern?: string): string {
  // JSON.stringify is the right tool here specifically because this gets
  // embedded as a JS expression inside <script> (assigned to a const, see
  // below) — JSON string syntax IS valid JS string-literal syntax with
  // correct escaping. It is NOT safe to bake directly into an HTML
  // attribute (see the fix note further down for exactly what went wrong
  // when this used to do that).
  const initialPatternJson = JSON.stringify(initialPattern ?? "");

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

  // ROOT CAUSE (2026-08-12) of "the tester keeps stray quotes and never
  // finds matches": this used to be baked into the <input value=...> HTML
  // attribute directly. JSON.stringify(str) always wraps its output in
  // literal quote characters -- that's correct JSON-string syntax, and
  // exactly what you want assigning into a JS string literal (this spot,
  // right here) -- but it's the wrong tool for an HTML attribute, which
  // needs the RAW characters HTML-entity-escaped instead, no wrapping
  // quotes added. Used directly as an HTML attribute value before, the
  // literal quote characters JSON.stringify adds became part of the actual
  // input value once the browser parsed the attribute -- every pre-filled
  // pattern silently gained a leading and trailing ", which regex.Regex
  // then dutifully required literal " characters in the test text to
  // match, so nothing ever did. Setting it as a real JS property here
  // instead sidesteps HTML-attribute escaping entirely -- no wrapping
  // quotes end up in the value no matter what the pattern contains.
  const initialPattern = ${initialPatternJson};
  if (initialPattern) { patternEl.value = initialPattern; }

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
    if (msg.type === 'setPattern') {
      patternEl.value = msg.pattern;
      run();
      return;
    }
    if (msg.type !== 'result') return;
    // 2026-08-09 — a stale/incompatible server (e.g. mdix-lsp failed to
    // rebuild after a compile error elsewhere in the crate, but an older
    // binary is still what's actually running) won't recognize
    // mdix.testRegex at all — tower-lsp's unknown-command fallback returns
    // Ok(None), which arrives here as result === null. render() used to
    // immediately do result.valid, throwing inside this listener with no
    // visible error anywhere — looked exactly like "never finds matches"
    // rather than the real problem (wrong/stale server). Checking for a
    // well-formed result before ever touching .valid.
    //
    // (Side note for whoever reads this comment next: NO backtick
    // characters in any comment anywhere inside this function's returned
    // template literal, ever, including this one — the whole HTML/script
    // block from the DOCTYPE line to the closing html tag is one giant
    // backtick-delimited TS string, and a backtick anywhere inside it —
    // even inside what looks like a harmless nested JS comment — closes
    // that outer literal right there and turns everything after it into
    // real TypeScript the compiler then tries and fails to parse. That's
    // exactly what happened here before: a markdown-style code-emphasis
    // backtick pair around "result.valid" in this very comment terminated
    // the literal mid-file and cascaded into a syntax error several dozen
    // lines away from the actual mistake. Use single quotes for emphasis
    // in here instead, never backticks — not even once, not even for a
    // one-word aside.
    if (!msg.result || typeof msg.result.valid !== 'boolean') {
      statusEl.innerHTML = '<span class="error">No usable response from the language server '
        + '(got: ' + JSON.stringify(msg.result) + '). If this persists, the running mdix-lsp '
        + 'may be stale or failed to rebuild — try DixScript: Restart Language Server.</span>';
      resultEl.textContent = '';
      return;
    }
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

  // Run immediately if this panel was opened pre-filled (from the CodeLens),
  // so results show up without requiring the user to type anything first.
  if (patternEl.value) { run(); }
</script>
</body>
</html>`;
}
