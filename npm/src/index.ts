// Re-export WASM-generated bindings.
// wasm-pack writes these into wasm-pkg/ during the build step.
export { MdixDatabase, MdixBuilder } from "../wasm-pkg/mdix_wasm.js";

// Re-export TypeScript layer.
export type { MdixValueType, MdixResult, MdixOk, MdixErr, MdixFormatMode } from "./types.js";
export { ok, err, tryGet, tryGetAsync, unwrap, unwrapOr } from "./result.js";
