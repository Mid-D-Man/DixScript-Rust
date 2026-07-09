// mdix-bootstrap.js
//
// Loads the mdix-wasm "--target web" build and publishes it globally so
// MdixWeb.jslib's plain (non-module) Emscripten library functions can reach
// it via `window.__mdixWasm`, and so C# (MdixWebGLReady.IsReady /
// WaitUntilReady) can tell when it's safe to call Dix.LoadStr.
//
// ── SETUP ─────────────────────────────────────────────────────────────────
//   1. Build the "web" target of mdix-wasm (an ES module + .wasm file that
//      works from a plain static file server — no bundler needed):
//
//        wasm-pack build mdix-wasm --target web --release \
//          --out-dir <your-webgl-template>/mdix-wasm-web \
//          --out-name mdix_wasm
//
//      This is the exact same invocation the repo's own CI already runs for
//      the GitHub Pages playground (.github/workflows/wasm-npm-publish.yml,
//      "build web target" step) — you can grab the wasm-web/ folder from
//      that workflow's uploaded build artifact instead of building it
//      yourself if you'd rather not install wasm-pack locally.
//
//   2. Copy THIS file into your WebGL template folder, next to (i.e. as a
//      sibling of) the mdix-wasm-web/ folder from step 1. A typical layout:
//
//        Assets/WebGLTemplates/MyTemplate/
//          index.html
//          mdix-bootstrap.js        <- this file
//          mdix-wasm-web/
//            mdix_wasm.js
//            mdix_wasm_bg.wasm
//
//   3. In index.html's <head>, BEFORE the Unity loader <script> tag, add:
//
//        <script type="module" src="mdix-bootstrap.js"></script>
//
//   4. Player Settings -> Resolution and Presentation -> WebGL Template ->
//      select your custom template.
//
// If your layout differs, change MDIX_WASM_URL below — it's resolved
// relative to this file's own location (ES module import semantics), not
// relative to index.html.

const MDIX_WASM_URL = './mdix-wasm-web/mdix_wasm.js';

window.__mdixReady = false;
window.__mdixWasm = null;

import(MDIX_WASM_URL)
  .then((mod) => mod.default().then(() => mod))
  .then((mod) => {
    // `mod` now exposes the wasm-bindgen-generated API from
    // mdix-wasm/src/lib.rs / database.rs — MdixDatabase, prefetchImport, etc.
    window.__mdixWasm = mod;
    window.__mdixReady = true;
    console.log('[MDIX] mdix-wasm bridge ready.');
  })
  .catch((err) => {
    console.error(
      '[MDIX] Failed to load mdix-wasm from "' + MDIX_WASM_URL + '". ' +
      'Check that mdix_wasm.js and mdix_wasm_bg.wasm exist at that path ' +
      'relative to mdix-bootstrap.js — see the setup comment at the top ' +
      'of this file.',
      err
    );
  });
