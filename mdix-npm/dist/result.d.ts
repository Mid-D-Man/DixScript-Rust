import type { MdixResult, MdixOk, MdixErr } from "./types.js";
/** Wraps a value in a successful result. */
export declare function ok<T>(value: T): MdixOk<T>;
/** Wraps an error message in a failed result. */
export declare function err(error: unknown): MdixErr;
/**
 * Wraps a function that may throw into a MdixResult.
 * Use this to call WASM methods without a try/catch at every call site.
 *
 * ```ts
 * const result = tryGet(() => db.getString("server.host"));
 * if (result.ok) console.log(result.value);
 * else           console.error(result.error);
 * ```
 */
export declare function tryGet<T>(fn: () => T): MdixResult<T>;
/**
 * Async variant of tryGet for future async WASM operations.
 */
export declare function tryGetAsync<T>(fn: () => Promise<T>): Promise<MdixResult<T>>;
/** Unwraps a result, throwing if it is an error. */
export declare function unwrap<T>(result: MdixResult<T>): T;
/** Returns the value or a fallback if the result is an error. */
export declare function unwrapOr<T>(result: MdixResult<T>, fallback: T): T;
//# sourceMappingURL=result.d.ts.map