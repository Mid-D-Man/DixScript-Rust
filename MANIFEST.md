# Round 4 — package.json/README updated, samples added. dixscript/ still untouched.

12 files: 3 mdix-lsp (unchanged since round 3), 9 mdix-vscode.

## New this round
- `mdix-vscode/package.json` — version 1.0.0 -> 1.1.0, description + keywords
  mention theme sync / settings sync
- `mdix-vscode/README.md` — Features list gets the two new commands, new
  "## Samples" section
- `mdix-vscode/samples/hello.mdix` — NEW. Core literal types tour (string,
  number, bool, array, hex color, date, timestamp) — exercises the inline
  color picker and 📅 date lens
- `mdix-vscode/samples/regex-and-blob.mdix` — NEW. Regex validation in
  @DATA (works today) + a blob you can preview with the ▶ lens. Uses \\w
  (doubled backslash) deliberately — see the comment in the file, a bare \w
  would silently lose the backslash during string-escape processing.

## Unchanged since round 3 (included for completeness)
- mdix-lsp/src/features/commands.rs, code_lens.rs, server.rs
- mdix-vscode/src/themeColors.ts, settingsSync.ts, extension.ts
- mdix-vscode/-_-master_colors.mdix, -_-master_settings.mdix

## Still not included (confirmed, per your instruction)
- Anything under dixscript/ — the regex-in-QuickFuncs parser/analyzer fix is
  reverted and shelved until you branch it.

## Noticed, not touched (flagged in chat, your call)
- mdix-vscode/.rmv/config/*.json (incl. an unrelated log4j2.properties) and
  mdix-vscode/Users/midman/Desktop/DixScript-Rust/... — both tracked in
  commit 21a4ca8, look like accidental debris from a folder upload.
