/** Mirrors MdixValueType from the C# layer. */
export type MdixValueType = "unknown" | "null" | "bool" | "int" | "long" | "float" | "double" | "string" | "date" | "timestamp" | "hex_color" | "blob" | "regex" | "array" | "object" | "tuple" | "enum";
/** A successful result. */
export interface MdixOk<T> {
    readonly ok: true;
    readonly value: T;
}
/** A failed result. */
export interface MdixErr {
    readonly ok: false;
    readonly error: string;
}
/**
 * Result type mirroring MdixResult<T> from the C# layer.
 * Lets callers avoid try/catch when using the safe wrappers
 * in result.ts.
 */
export type MdixResult<T> = MdixOk<T> | MdixErr;
/** Options accepted by the format utilities. */
export type MdixFormatMode = "default" | "pretty" | "compact" | "minified";
/**
 * Conflict-resolution strategy accepted by `mergeSources`,
 * `mergeSourcesWeighted`, and `MdixDatabase.mergeWith`.
 * Defaults to `"weighted"` when omitted.
 */
export type MdixMergeStrategy = "weighted" | "primary_wins" | "secondary_wins" | "throw_on_conflict";
/**
 * Array-merge strategy accepted alongside `MdixMergeStrategy` by the same
 * three call sites. Defaults to `"concat_dedup"` when omitted.
 */
export type MdixArrayMergeStrategy = "replace" | "concat" | "concat_dedup";
/**
 * Shape of each entry in the array returned by `MdixMergeOutcome.conflicts()`.
 * The raw wasm binding returns `any` (it's parsed from JSON on the Rust
 * side) — cast to `MdixMergeConflict[]` when you need the fields typed:
 * ```ts
 * const conflicts = outcome.conflicts() as MdixMergeConflict[];
 * ```
 */
export interface MdixMergeConflict {
    readonly path: string;
    readonly winningSource: string;
    readonly winningLabel: string;
}
/** The kind of failure in a `MdixValidationReport` error entry. */
export type MdixValidationErrorKind = "Missing" | "WrongType" | "InvalidValue";
/**
 * Shape of each entry in the array returned by `MdixValidationReport.errors()`.
 * Same casting note as `MdixMergeConflict` applies — the raw binding
 * returns `any`:
 * ```ts
 * const errors = report.errors() as MdixValidationError[];
 * ```
 */
export interface MdixValidationError {
    readonly path: string;
    readonly expected: string;
    readonly actual: string;
    readonly kind: MdixValidationErrorKind;
}
//# sourceMappingURL=types.d.ts.map