<!-- com.midmanstudio.mdix.webgl/README.md -->

# com.midmanstudio.mdix.webgl

Unity WebGL support for `com.midmanstudio.mdix`. Ships separately from the
core package (see "Why a separate package" below) — install both.

**Status: first vertical slice, not yet build-tested.** so nothing below has been verified in a real WebGL build yet.
Validate this slice for real before building anything further on top of it —
see "What's next" at the bottom.

---

## What this does

`mdix-ffi` (the native P/Invoke backend `MdixDatabase` uses on every other
platform) cannot be linked into a WebGL player — Unity WebGL statically
links the whole game into one Emscripten module at build time, and there is
no `wasm32-unknown-emscripten` build of `mdix-ffi` (`mdix-ffi/Cargo.toml`
already documents this as an explicit non-goal).

`mdix-wasm` (a *different* target, `wasm32-unknown-unknown`, built via
`wasm-pack`) already runs fine in a browser and already ships a
`--target web` build for exactly this kind of use (the repo's own GitHub
Pages playground uses it). This package bridges `MdixDatabase` to that build
via a `.jslib` shim.

## Why a separate package

Everything in this package is purely additive — it required **zero changes**
to `com.midmanstudio.mdix` or `mdix-csharp`. Keeping it separate means:
- Projects that don't target WebGL never pull in a `.jslib`, a WebGL-only
  `.asmdef`, or the bootstrap script.
- The core package's `MidManStudio.Mdix.Core.dll` stays exactly what it
  already is: a plain, portable netstandard2.1 library, also published
  standalone as a NuGet package for non-Unity .NET consumers. It has no idea
  Unity or WebGL exist, and — see below — architecturally it *can't* know.

## Why this isn't `#if UNITY_WEBGL` inside `MdixDatabase.cs`

That was the original plan and it doesn't work, for a reason worth writing
down so it doesn't get re-attempted by accident:

`MidManStudio.Mdix.Core.dll` (the DLL `com.midmanstudio.mdix`'s `.asmdef`
references via `precompiledReferences`) is built **once**, generically, by
`.github/workflows/build-upm.yml`'s `build-core-dll` job — a plain
`dotnet build csharp/src/MidManStudio.Mdix.Core/MidManStudio.Mdix.Core.csproj`
with no `-p:DefineConstants`. `UNITY_WEBGL`, `UNITY_EDITOR`, `UNITY_IOS` etc.
are symbols Unity's *own* compiler defines when it compiles a project's
`Assets`/package `Runtime` folders directly — `dotnet build` has never heard
of them. Any `#if UNITY_WEBGL` written inside `mdix-csharp` would always
silently take the `#else` branch in the shipped DLL, on every platform,
including WebGL.

(Side finding, unrelated to this package, not fixed here: the *generated*
`MdixNative.cs` already has exactly this problem today. `mdix-ffi/build.rs`
calls `csbindgen::Builder::csharp_dll_name_if("UNITY_IOS && !UNITY_EDITOR", "__Internal")`
so that iOS's statically-linked `libmdix_ffi.a` resolves via `__Internal`
instead of trying to `dlopen("mdix_ffi")`. Since `UNITY_IOS` is never defined
at the point this file is actually compiled (same `build-core-dll` job,
same plain `dotnet build`), every `[DllImport]` in the shipped DLL uses
`"mdix_ffi"` on every platform — including iOS, where that will fail. Worth
a look independently of WebGL; flagging it here since it's the same root
cause.)

The fix that *does* work, used here: on WebGL, IL2CPP resolves
`[DllImport]` externs **by function name** against whatever `.jslib`
symbols got merged in at build time — the library-name string
(`"mdix_ffi"`) isn't used for resolution the way it is on platforms with
real dynamic linking. So `MdixWeb.jslib` implements JS functions with the
exact same names as `mdix-ffi`'s real C ABI (`mdix_load_str`, `mdix_free`,
`mdix_get_string`, `mdix_get_int`, `mdix_get_bool`, `mdix_exists`,
`mdix_entry_count`, `mdix_free_string`, `mdix_get_last_error`,
`mdix_clear_error`) — and the *existing, unmodified* calls inside
`MdixDatabase.cs`/`MdixSafeHandle.cs` get wired straight into them on WebGL,
with zero code in `mdix-csharp` aware this is happening.

## What's covered

| MdixDatabase / Dix API | WebGL |
|---|---|
| `LoadStr` / `Dix.LoadStr` | ✅ |
| `GetString` | ✅ |
| `GetInt` | ✅ |
| `GetBool` | ✅ |
| `Exists` | ✅ |
| `EntryCount` | ✅ |
| `IsValid`, `Dispose` | ✅ (no native call needed) |
| Everything else (`Load` from path, `LoadEncrypted*`, `GetLong/Float/Double/Json`, arrays, tuples, enums, hot reload, schema validation, `MdixConverter`/POCO, merge, builder) | ❌ not ported |

Calling anything in the "not ported" row from code that ships in a WebGL
build will fail **at Unity's IL2CPP link step** (`undefined symbol:
_mdix_xxx`), not at runtime — there's no native binary and no matching
`.jslib` entry for those symbols yet. Keep a WebGL smoke-test scene to just
the ✅ row until this is expanded. To add one: find the function's exact
signature and error-handling contract in `mdix-ffi/src/lib.rs`, then add a
matching entry to `MdixWeb.jslib` following the existing ones as a template.

## Setup

1. **Build mdix-wasm's `--target web` output.**
   ```
   wasm-pack build mdix-wasm --target web --release \
     --out-dir <your-webgl-template>/mdix-wasm-web --out-name mdix_wasm
   ```
   This is the same invocation `.github/workflows/wasm-npm-publish.yml`
   already runs for the GitHub Pages playground — you can pull the
   `wasm-web/` folder from that workflow's uploaded build artifact instead
   of building it yourself.

2. **Set up a custom WebGL template** (Unity requires this to inject the
   bootstrap script — there's no supported way to add a `<script>` tag to
   the default template):
   - Create `Assets/WebGLTemplates/MdixWebGL/` in your project (copy
     Unity's default WebGL template as a starting point if you don't
     already have a custom one — see Unity's WebGL custom template docs for
     the boilerplate `index.html` needs, since that's Unity-version-specific
     and deliberately not duplicated here).
   - Copy `Runtime/WebGLTemplate/mdix-bootstrap.js` from this package into
     that folder.
   - Copy the `mdix-wasm-web/` folder from step 1 into that same folder, so
     `mdix-bootstrap.js` sits right next to it.
   - In `index.html`'s `<head>`, **before** the Unity loader `<script>` tag:
     ```html
     <script type="module" src="mdix-bootstrap.js"></script>
     ```
   - Player Settings → Resolution and Presentation → WebGL Template →
     select `MdixWebGL`.

3. **Wait for readiness before your first load call:**
   ```csharp
   using System.Collections;
   using MidManStudio.Mdix;
   using MidManStudio.Mdix.WebGL;

   IEnumerator Start()
   {
       yield return MdixWebGLReady.WaitUntilReady();
       if (!MdixWebGLReady.IsReady) yield break; // timed out, already logged

       var result = Dix.LoadStr(mySource);
       if (result.IsSuccess)
       {
           var db = result.SuccessResult;
           Debug.Log(db.GetString("some.path").SuccessResult);
       }
   }
   ```
   `MdixWebGLReady` also starts polling automatically before the first scene
   loads, so a call in an early `Awake()` doesn't lose the first few frames
   — but you still need to `yield return WaitUntilReady()` (or poll
   `IsReady`) yourself before the actual `LoadStr` call.

## What's next

- Build-test this exact slice in a real Unity WebGL build (browser +
  headless or manual) before expanding coverage — this is the point where
  real surprises (Asyncify settings if any are needed, template quirks,
  IL2CPP-specific marshaling edge cases, whether plain `0`/`1` JS returns
  really do marshal cleanly to the generated `MdixNative.cs`'s exact `bool`
  return signature) would show up cheapest to debug on a small slice, per
  the original plan.
- Once validated: expand `MdixWeb.jslib` toward `GetLong`/`GetFloat`/
  `GetDouble`/`GetJson`/enums/arrays next (all single-value getters,
  same shape as the ones already done) before tackling the stateful
  pieces (hot reload's `FileSystemWatcher` has no browser equivalent at
  all and needs a real design, not just a shim; builder/merge need their
  own registry-object story on the mdix-wasm side first).
- Wire a CI step (likely a small addition to `wasm-npm-publish.yml`, since
  it already builds the `--target web` output) to drop `mdix-wasm-web/`
  straight into this package instead of requiring a manual `wasm-pack`
  build — not done here since it's tooling around an unvalidated slice.
- Look at the iOS `__Internal` dead-code finding above independently.
