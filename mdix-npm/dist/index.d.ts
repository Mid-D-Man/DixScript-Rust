export { MdixBuilder, MdixDatabase, prefetchImport, compileWithDlm, decompileWithDlm, MdixDlmOutcome, mergeSources, mergeSourcesWeighted, MdixMergeOutcome, MdixSchema, MdixValidationReport, MdixWatcher, MdixWatchOutcome, } from "../wasm-pkg/mdix_wasm.js";
export type { MdixValueType, MdixResult, MdixOk, MdixErr, MdixFormatMode, MdixMergeStrategy, MdixArrayMergeStrategy, MdixMergeConflict, MdixValidationErrorKind, MdixValidationError, } from "./types.js";
export { ok, err, tryGet, tryGetAsync, unwrap, unwrapOr } from "./result.js";
export { query, queryMany } from "./query.js";
//# sourceMappingURL=index.d.ts.map