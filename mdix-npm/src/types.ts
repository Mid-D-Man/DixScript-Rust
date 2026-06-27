// mdix-npm/src/types.ts

/** Mirrors MdixValueType from the C# layer. */
export type MdixValueType =
  | "unknown"
  | "null"
  | "bool"
  | "int"
  | "long"
  | "float"
  | "double"
  | "string"
  | "date"
  | "timestamp"
  | "hex_color"
  | "blob"
  | "regex"
  | "array"
  | "object"
  | "tuple"
  | "enum";

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
