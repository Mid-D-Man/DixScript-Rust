/**
 * "Open Sample" command — browse and open the .mdix files bundled under
 * samples/ (hello.mdix, datetime_static_test.mdix, all_datatypes_test.mdix,
 * regex-and-blob.mdix) without hunting through the extension's install
 * folder for them.
 *
 * Each pick opens as a fresh untitled `mdix` buffer (a copy of the bundled
 * content), so nothing under the extension's own install directory ever
 * gets edited by accident.
 */

import * as fs   from "fs";
import * as path from "path";
import { ExtensionContext, QuickPickItem, commands, window, workspace } from "vscode";

interface SampleItem extends QuickPickItem {
  filePath: string;
}

const DESCRIPTIONS: Record<string, string> = {
  "hello.mdix": "Core literal types — strings, numbers, booleans, arrays.",
  "datetime_static_test.mdix": "@DateTime literals and the static editor widget.",
  "all_datatypes_test.mdix": "A tour of every DixScript datatype in one file.",
  "regex-and-blob.mdix": "Regex validation, plus blob sniffing/preview for images and audio.",
};

export function registerSampleBrowser(context: ExtensionContext): void {
  context.subscriptions.push(
    commands.registerCommand("dixscript.openSample", async () => {
      const samplesDir = path.join(context.extensionPath, "samples");

      let files: string[];
      try {
        files = fs
          .readdirSync(samplesDir)
          .filter(f => f.endsWith(".mdix"))
          .sort();
      } catch {
        window.showErrorMessage("DixScript: no bundled samples found.");
        return;
      }

      if (files.length === 0) {
        window.showErrorMessage("DixScript: no bundled samples found.");
        return;
      }

      const items: SampleItem[] = files.map(f => ({
        label: f,
        description: DESCRIPTIONS[f] ?? "",
        filePath: path.join(samplesDir, f),
      }));

      const picked = await window.showQuickPick(items, {
        title: "DixScript: Open Sample",
        placeHolder: "Pick a sample .mdix file to open",
      });
      if (!picked) {
        return;
      }

      let content: string;
      try {
        content = fs.readFileSync(picked.filePath, "utf8");
      } catch (err) {
        window.showErrorMessage(`DixScript: couldn't read ${picked.label} (${String(err)}).`);
        return;
      }

      const doc = await workspace.openTextDocument({ language: "mdix", content });
      await window.showTextDocument(doc, { preview: false });
    })
  );
}
