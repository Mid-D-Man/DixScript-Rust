// Node-specific entry point. Exists for one reason: wasm-pack's
// --target bundler output (index.ts's source) contains a raw
// `import * as X from "./mdix_wasm_bg.wasm"` — that's meant to be
// resolved by an actual bundler (webpack/vite/rollup) that knows how to
// turn a .wasm import into something loadable. Plain Node has no idea
// what to do with it and throws ERR_UNKNOWN_FILE_EXTENSION before your
// code even runs (confirmed directly against a real CI run).
//
// wasm-pack's --target nodejs output solves this a completely different
// way: it loads the .wasm file itself via `fs.readFileSync` +
// `WebAssembly.Instance`, synchronously, no ESM .wasm import involved.
// That's the only wasm-pack target actually meant for direct
// `node script.mjs` execution with no bundler in the loop -- see
// package.json's build:wasm script, which builds both targets, and its
// "node" export condition, which points real `import ... from
// "@dixscript/core"` consumers here under plain Node.
//
// This file is intentionally identical to index.ts below the import
// line -- same symbols, same shape, same re-exported TS layer. Keep
// them in sync by hand; there are only two lines that differ (the wasm
// import source and this file's own header).
export {
  // ── Builder ─────────────────────────────────────────────────────────────
  MdixBuilder,

  // ── Database ────────────────────────────────────────────────────────────
  MdixDatabase,
  prefetchImport,

  // ── DLM (compress / encrypt / audit) ───────────────────────────────────
  compileWithDlm,
  decompileWithDlm,
  MdixDlmOutcome,

  // ── Merge ───────────────────────────────────────────────────────────────
  mergeSources,
  mergeSourcesWeighted,
  MdixMergeOutcome,

  // ── Schema validation ───────────────────────────────────────────────────
  MdixSchema,
  MdixValidationReport,

  // ── Hot reload / watch ──────────────────────────────────────────────────
  MdixWatcher,
  MdixWatchOutcome,
} from "../wasm-pkg-node/mdix_wasm.js";

// Re-export TypeScript layer (identical to index.ts).
export type {
  MdixValueType,
  MdixResult,
  MdixOk,
  MdixErr,
  MdixFormatMode,
  MdixMergeStrategy,
  MdixArrayMergeStrategy,
  MdixMergeConflict,
  MdixValidationErrorKind,
  MdixValidationError,
} from "./types.js";
export { ok, err, tryGet, tryGetAsync, unwrap, unwrapOr } from "./result.js";
export { query, queryMany } from "./query.js";
