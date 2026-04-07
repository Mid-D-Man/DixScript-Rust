import type { MdixResult, MdixOk, MdixErr } from "./types.js";

/** Wraps a value in a successful result. */
export function ok<T>(value: T): MdixOk<T> {
  return { ok: true, value };
}

/** Wraps an error message in a failed result. */
export function err(error: unknown): MdixErr {
  const message =
    error instanceof Error
      ? error.message
      : typeof error === "string"
      ? error
      : String(error);
  return { ok: false, error: message };
}

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
export function tryGet<T>(fn: () => T): MdixResult<T> {
  try {
    return ok(fn());
  } catch (e) {
    return err(e);
  }
}

/**
 * Async variant of tryGet for future async WASM operations.
 */
export async function tryGetAsync<T>(
  fn: () => Promise<T>
): Promise<MdixResult<T>> {
  try {
    return ok(await fn());
  } catch (e) {
    return err(e);
  }
}

/** Unwraps a result, throwing if it is an error. */
export function unwrap<T>(result: MdixResult<T>): T {
  if (result.ok) return result.value;
  throw new Error(result.error);
}

/** Returns the value or a fallback if the result is an error. */
export function unwrapOr<T>(result: MdixResult<T>, fallback: T): T {
  return result.ok ? result.value : fallback;
    }
