/**
 * Preview widget for DixScript `b:(...)` blob literals.
 *
 * `mdix-lsp`'s CodeLens provider emits a "▶ Preview blob" lens over every
 * `BlobConstructor` token, passing the document URI and the blob's raw
 * base64 content (the string literal inside the parens) as arguments to
 * `mdix.previewBlob`. Everything else — base64 decode, content sniffing,
 * and rendering — happens right here in the extension; no LSP round-trip
 * or Rust-side decoding is needed since the blob content is already plain
 * base64 text sitting in the token.
 *
 * Registered client-side only (same reasoning as dateTimeEditor.ts).
 */

import * as vscode from "vscode";

export function registerBlobPreview(context: vscode.ExtensionContext): void {
  context.subscriptions.push(
    vscode.commands.registerCommand(
      "mdix.previewBlob",
      (_uriStr: string, base64Content: string) => {
        openBlobPreview(base64Content);
      }
    )
  );
}

type MediaCategory = "image" | "audio" | "video" | "text" | "unknown";

interface Sniffed {
  category: MediaCategory;
  mime: string;
  label: string;
}

function openBlobPreview(base64Content: string): void {
  let bytes: Buffer;
  try {
    bytes = Buffer.from(base64Content, "base64");
  } catch {
    vscode.window.showErrorMessage("DixScript: blob content isn't valid base64.");
    return;
  }

  const info = sniff(bytes);

  const panel = vscode.window.createWebviewPanel(
    "mdixBlobPreview",
    `Blob Preview — ${info.label}`,
    { viewColumn: vscode.ViewColumn.Beside, preserveFocus: false },
    { enableScripts: false }
  );

  panel.webview.html = buildHtml(bytes, info, base64Content);
}

// ── Content sniffing ──────────────────────────────────────────────────────────
//
// Deliberately simple magic-byte checks covering the common cases a DixScript
// blob is likely to hold. Falls through to a printable-text heuristic, then
// a raw hex dump for anything unrecognized — never blocks on an unknown type.

function sniff(bytes: Buffer): Sniffed {
  const startsWith = (sig: number[], offset = 0): boolean =>
    bytes.length >= offset + sig.length && sig.every((b, i) => bytes[offset + i] === b);

  const ascii = (offset: number, len: number): string =>
    bytes.length >= offset + len ? bytes.toString("ascii", offset, offset + len) : "";

  const img = (mime: string, label: string): Sniffed => ({ category: "image", mime, label });
  const aud = (mime: string, label: string): Sniffed => ({ category: "audio", mime, label });
  const vid = (mime: string, label: string): Sniffed => ({ category: "video", mime, label });

  if (startsWith([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])) return img("image/png", "PNG image");
  if (startsWith([0xff, 0xd8, 0xff]))                               return img("image/jpeg", "JPEG image");
  if (ascii(0, 6) === "GIF87a" || ascii(0, 6) === "GIF89a")         return img("image/gif", "GIF image");
  if (startsWith([0x42, 0x4d]))                                     return img("image/bmp", "BMP image");
  if (ascii(0, 4) === "RIFF" && ascii(8, 4) === "WEBP")              return img("image/webp", "WebP image");
  if (ascii(0, 4) === "RIFF" && ascii(8, 4) === "WAVE")              return aud("audio/wav", "WAV audio");
  if (ascii(0, 4) === "fLaC")                                        return aud("audio/flac", "FLAC audio");
  if (ascii(0, 4) === "OggS")                                        return aud("audio/ogg", "Ogg audio");
  if (ascii(0, 3) === "ID3" || (bytes.length > 1 && bytes[0] === 0xff && (bytes[1] & 0xe0) === 0xe0)) {
    return aud("audio/mpeg", "MP3 audio");
  }
  if (startsWith([0x1a, 0x45, 0xdf, 0xa3]))                          return vid("video/webm", "WebM/Matroska video");
  if (ascii(4, 4) === "ftyp") {
    const brand = ascii(8, 4);
    if (brand.startsWith("M4A")) return aud("audio/mp4", "M4A audio");
    return vid("video/mp4", "MP4 video");
  }
  if (ascii(0, 4) === "%PDF") {
    return { category: "unknown", mime: "application/pdf", label: "PDF document" };
  }

  // Printable-text heuristic: sample the first 512 bytes, require ≥95%
  // printable ASCII or common whitespace before calling it "text".
  const sample = bytes.subarray(0, Math.min(512, bytes.length));
  let printable = 0;
  for (const byte of sample) {
    if ((byte >= 0x20 && byte <= 0x7e) || byte === 0x09 || byte === 0x0a || byte === 0x0d) {
      printable++;
    }
  }
  if (sample.length > 0 && printable / sample.length > 0.95) {
    return { category: "text", mime: "text/plain", label: "Text content" };
  }

  return { category: "unknown", mime: "application/octet-stream", label: "Unknown binary data" };
}

function hexDump(bytes: Buffer, maxBytes = 256): string {
  const slice = bytes.subarray(0, maxBytes);
  const lines: string[] = [];
  for (let i = 0; i < slice.length; i += 16) {
    const chunk = slice.subarray(i, i + 16);
    const hex = Array.from(chunk).map(b => b.toString(16).padStart(2, "0")).join(" ");
    const asciiCol = Array.from(chunk)
      .map(b => (b >= 0x20 && b <= 0x7e ? String.fromCharCode(b) : "."))
      .join("");
    lines.push(`${i.toString(16).padStart(6, "0")}  ${hex.padEnd(47)}  ${asciiCol}`);
  }
  return lines.join("\n");
}

// ── Rendering ─────────────────────────────────────────────────────────────────

function buildHtml(bytes: Buffer, info: Sniffed, base64Content: string): string {
  const dataUri = `data:${info.mime};base64,${base64Content}`;
  const sizeLabel = `${bytes.length.toLocaleString()} bytes`;

  let mediaHtml: string;
  switch (info.category) {
    case "image":
      mediaHtml = `<img src="${dataUri}" alt="blob preview" />`;
      break;
    case "audio":
      mediaHtml = `<audio controls src="${dataUri}"></audio>`;
      break;
    case "video":
      mediaHtml = `<video controls src="${dataUri}"></video>`;
      break;
    case "text":
      mediaHtml = `<pre class="dump">${escapeHtml(bytes.toString("utf8").slice(0, 4000))}</pre>`;
      break;
    default: {
      const dump = hexDump(bytes);
      const truncated = bytes.length > 256 ? "\n…" : "";
      mediaHtml = `<pre class="dump">${escapeHtml(dump)}${truncated}</pre>`;
    }
  }

  return `<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8">
<style>
  body {
    font-family: var(--vscode-font-family);
    color: var(--vscode-foreground);
    background: var(--vscode-editor-background);
    padding: 20px;
  }
  .meta {
    opacity: 0.75;
    font-size: 12px;
    margin-bottom: 14px;
  }
  img, video {
    max-width: 100%;
    border-radius: 4px;
  }
  audio, video {
    width: 100%;
  }
  .dump {
    font-family: var(--vscode-editor-font-family, monospace);
    font-size: 12px;
    background: var(--vscode-textCodeBlock-background, rgba(128,128,128,0.1));
    padding: 12px;
    border-radius: 4px;
    overflow-x: auto;
    white-space: pre;
  }
</style>
</head>
<body>
  <div class="meta">${info.label} · ${sizeLabel} · ${info.mime}</div>
  ${mediaHtml}
</body>
</html>`;
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
    }
