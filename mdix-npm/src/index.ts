// Re-export WASM-generated bindings.
// wasm-pack writes these into wasm-pkg/ during the build step.
//
// This mirrors mdix-wasm/src/lib.rs's full `pub use` list exactly — that
// file is the source of truth for what this package should surface.
// Previously only MdixDatabase/MdixBuilder were re-exported here, which
// left DLM compile/decompile, AST merge, schema validation, and the
// content-hash watcher completely unreachable from npm even though the
// compiled wasm binary contained them either way.
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
} from "../wasm-pkg/mdix_wasm.js";

// Re-export TypeScript layer.
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
