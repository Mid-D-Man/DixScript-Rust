/**
 * "Apply Theme" — reads a DixScript `dark:`/`light:` color table (see
 * `-_-master_colors.mdix` for the expected shape) and writes it into VS
 * Code's `editor.semanticTokenColorCustomizations` setting, scoped to the
 * currently active color theme by name.
 *
 * Registered client-side (same pattern as `dateTimeEditor.ts` /
 * `blobPreview.ts`) so vscode-languageclient intercepts CodeLens clicks for
 * `mdix.applyThemeColors` locally instead of forwarding them to the server.
 * The actual parsing still happens server-side, via `mdix.getThemeColors`
 * (declared in `executeCommandProvider`, see capabilities.rs) — this file is
 * only the VS Code API half: fetch the JSON, pick the dark/light half for
 * the active theme kind, merge it into the setting, report the result.
 *
 * Deliberately NOT scoped to "any dark theme" / "any light theme" generically:
 * VS Code has no such wildcard for `editor.semanticTokenColorCustomizations`
 * (see https://github.com/microsoft/vscode/issues/194555 — `[Name*]` only
 * prefix-matches a theme's *name*, not its kind/appearance). Scoping to the
 * literal active theme name instead means running this once per theme you
 * actually use is enough going forward — no need to re-run on every switch
 * between a light and a dark theme.
 */

import * as vscode from "vscode";
import { LanguageClient } from "vscode-languageclient/node";

const CMD_APPLY_THEME_COLORS = "mdix.applyThemeColors";
const CMD_GET_THEME_COLORS   = "mdix.getThemeColors";
const SETTING_KEY            = "editor.semanticTokenColorCustomizations";
const MAX_WARNINGS_SHOWN     = 5;

interface ThemeColorsResult {
  success:  boolean;
  message:  string;
  dark:     Record<string, string> | null;
  light:    Record<string, string> | null;
  warnings: string[];
}

export function registerThemeColors(
  context: vscode.ExtensionContext,
  getClient: () => LanguageClient | undefined
): void {
  context.subscriptions.push(
    vscode.commands.registerCommand(
      CMD_APPLY_THEME_COLORS,
      async (uriStr?: string) => {
        await applyThemeColors(getClient(), uriStr);
      }
    )
  );
}

async function applyThemeColors(
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
      "DixScript: open the .mdix file with your dark:/light: color tables, then run this again."
    );
    return;
  }

  let result: ThemeColorsResult;
  try {
    result = (await client.sendRequest("workspace/executeCommand", {
      command:   CMD_GET_THEME_COLORS,
      arguments: [uri.toString()],
    })) as ThemeColorsResult;
  } catch (err) {
    vscode.window.showErrorMessage(`DixScript: could not read theme colors — ${describeError(err)}`);
    return;
  }

  if (!result || !result.success) {
    vscode.window.showErrorMessage(`DixScript: ${result?.message ?? "no theme data returned."}`);
    return;
  }

  const kind    = vscode.window.activeColorTheme.kind;
  const isLight = kind === vscode.ColorThemeKind.Light || kind === vscode.ColorThemeKind.HighContrastLight;
  const chosen  = isLight ? result.light : result.dark;

  if (!chosen) {
    const missing = isLight ? "light" : "dark";
    vscode.window.showWarningMessage(
      `DixScript: active theme is ${isLight ? "light" : "dark"}, but the file has no \`${missing}:\` table.`
    );
    return;
  }

  const themeName = vscode.workspace.getConfiguration().get<string>("workbench.colorTheme");
  if (!themeName) {
    vscode.window.showErrorMessage("DixScript: could not determine the active theme name.");
    return;
  }

  const config    = vscode.workspace.getConfiguration();
  const existing  = config.get<Record<string, unknown>>(SETTING_KEY) ?? {};
  const themeKey  = `[${themeName}]`;
  const merged    = {
    ...existing,
    [themeKey]: {
      ...(existing[themeKey] as Record<string, unknown> | undefined),
      rules: chosen,
    },
  };

  await config.update(SETTING_KEY, merged, vscode.ConfigurationTarget.Global);

  const colorCount = Object.keys(chosen).length;
  let summary = `DixScript: applied ${colorCount} color(s) to "${themeName}".`;
  if (result.warnings.length > 0) {
    summary += ` (${result.warnings.length} skipped)`;
  }
  vscode.window.showInformationMessage(summary);

  if (result.warnings.length > 0) {
    const shown = result.warnings.slice(0, MAX_WARNINGS_SHOWN).join("; ");
    const more  = result.warnings.length > MAX_WARNINGS_SHOWN
      ? ` (and ${result.warnings.length - MAX_WARNINGS_SHOWN} more)`
      : "";
    vscode.window.showWarningMessage(`DixScript theme: ${shown}${more}`);
  }
}

function resolveTargetUri(uriStr?: string): vscode.Uri | undefined {
  if (uriStr) {
    return vscode.Uri.parse(uriStr);
  }
  const active = vscode.window.activeTextEditor;
  if (active && active.document.languageId === "mdix") {
    return active.document.uri;
  }
  return undefined;
}

function describeError(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
