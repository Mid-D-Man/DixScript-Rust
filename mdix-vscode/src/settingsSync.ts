/**
 * "Apply Settings" — reads a DixScript `settings:` table (see
 * `-_-master_settings.mdix`) and bulk-applies it to a curated set of VS
 * Code / DixScript settings.
 *
 * Same architecture as themeColors.ts: registered client-side so
 * vscode-languageclient intercepts CodeLens clicks for `mdix.applySettings`
 * locally; the actual parsing happens server-side via `mdix.getSettingsValues`
 * (curated allowlist — see `map_setting_key` in commands.rs). This file only
 * applies the result.
 *
 * Each returned setting carries a `scope`:
 *   - "global" — a plain User setting (dixscript.server.trace, .extraArgs).
 *   - "mdix"   — an `editor.*` setting, written scoped to .mdix files only
 *     via VS Code's `overrideInLanguage` update flag (the official pattern —
 *     see microsoft/vscode-extension-samples' configuration-sample:
 *     `getConfiguration('', { languageId }).update(key, value, target, true)`)
 *     rather than every language you edit.
 */

import * as vscode from "vscode";
import { LanguageClient } from "vscode-languageclient/node";

const CMD_APPLY_SETTINGS      = "mdix.applySettings";
const CMD_GET_SETTINGS_VALUES = "mdix.getSettingsValues";
const MDIX_LANGUAGE_ID        = "mdix";
const MAX_WARNINGS_SHOWN      = 5;

interface SettingEntry {
  key:   string;
  scope: "global" | "mdix";
  value: unknown;
}

interface SettingsValuesResult {
  success:  boolean;
  message:  string;
  settings: SettingEntry[];
  warnings: string[];
}

export function registerSettingsSync(
  context: vscode.ExtensionContext,
  getClient: () => LanguageClient | undefined
): void {
  context.subscriptions.push(
    vscode.commands.registerCommand(
      CMD_APPLY_SETTINGS,
      async (uriStr?: string) => {
        await applySettings(getClient(), uriStr);
      }
    )
  );
}

async function applySettings(
  client: LanguageClient | undefined,
  uriStr?: string
): Promise<void> {
  if (!client) {
    vscode.window.showErrorMessage("DixScript: language server is not running.");
    return;
  }

  const uri = resolveTargetUri(uriStr);
  if (!uri) {
    vscode.window.showErrorMessage(
      "DixScript: open the .mdix file with your settings: table, then run this again."
    );
    return;
  }

  let result: SettingsValuesResult;
  try {
    result = (await client.sendRequest("workspace/executeCommand", {
      command:   CMD_GET_SETTINGS_VALUES,
      arguments: [uri.toString()],
    })) as SettingsValuesResult;
  } catch (err) {
    vscode.window.showErrorMessage(`DixScript: could not read settings — ${describeError(err)}`);
    return;
  }

  if (!result || !result.success) {
    vscode.window.showErrorMessage(`DixScript: ${result?.message ?? "no settings returned."}`);
    return;
  }

  let applied = 0;
  const failures: string[] = [];

  for (const entry of result.settings) {
    try {
      if (entry.scope === "mdix") {
        await vscode.workspace
          .getConfiguration("", { languageId: MDIX_LANGUAGE_ID })
          .update(entry.key, entry.value, vscode.ConfigurationTarget.Global, /* overrideInLanguage */ true);
      } else {
        await vscode.workspace
          .getConfiguration()
          .update(entry.key, entry.value, vscode.ConfigurationTarget.Global);
      }
      applied++;
    } catch (err) {
      failures.push(`${entry.key} — ${describeError(err)}`);
    }
  }

  let summary = `DixScript: applied ${applied}/${result.settings.length} setting(s).`;
  if (result.warnings.length > 0) {
    summary += ` (${result.warnings.length} skipped)`;
  }
  vscode.window.showInformationMessage(summary);

  const allProblems = [...result.warnings, ...failures];
  if (allProblems.length > 0) {
    const shown = allProblems.slice(0, MAX_WARNINGS_SHOWN).join("; ");
    const more  = allProblems.length > MAX_WARNINGS_SHOWN
      ? ` (and ${allProblems.length - MAX_WARNINGS_SHOWN} more)`
      : "";
    vscode.window.showWarningMessage(`DixScript settings: ${shown}${more}`);
  }
}

function resolveTargetUri(uriStr?: string): vscode.Uri | undefined {
  if (uriStr) {
    return vscode.Uri.parse(uriStr);
  }
  const active = vscode.window.activeTextEditor;
  if (active && active.document.languageId === MDIX_LANGUAGE_ID) {
    return active.document.uri;
  }
  return undefined;
}

function describeError(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
