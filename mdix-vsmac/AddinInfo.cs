using Mono.Addins;
using Mono.Addins.Description;
using MonoDevelop.Core;

[assembly: Addin(
    "MdixLanguageSupport",
    Namespace = "com.midmanstudio",
    Version   = "1.0"
)]
[assembly: AddinName("DixScript (.mdix) Language Support")]
[assembly: AddinCategory("Language bindings")]
[assembly: AddinDescription("Syntax highlighting, completions, diagnostics and more for .mdix files via mdix-lsp.")]
[assembly: AddinAuthor("MidManStudio")]
[assembly: AddinUrl("https://github.com/Mid-D-Man/DixScript-Rust")]

// ----------------------------------------------------------------------------
// 2026-08-03 — ROOT CAUSE of the 7KB .mpack (and the file-tree corruption
// when opening/expanding a folder with a .mdix file in it):
//
// vstool/mdtool's "setup pack" does NOT bundle everything sitting next to
// the DLL in the build output. Per Mono.Addins' own docs ("Files included in
// an add-in"): a file only gets packed if it's declared as belonging to the
// addin, either via <Import file="..."/> in a <Runtime> manifest section, or
// via this attribute, [assembly: ImportAddinFile]. Manifest.addin.xml never
// had a <Runtime> section, so packing only ever produced the DLL itself —
// no mdix-lsp binary, no icon SVGs. That's the entire 7KB.
//
// Confirmed fixed: unzip -l on the resulting .mpack showed lsp/mdix-lsp
// present at the correct nested path (57,100,400 bytes), so the
// ImportAddinFile declaration below is doing its job and the subdirectory
// survives packaging intact.
// ----------------------------------------------------------------------------
[assembly: ImportAddinFile("lsp/mdix-lsp")]

// 2026-08-06 — mdix (the mdix-cli crate's binary — see mdix-cli/Cargo.toml's
// [[bin]] name = "mdix") added alongside mdix-lsp, same fix as
// mdix-vscode's copy-binary.js. mdix-lsp's own CLI resolution (which_mdix()
// in mdix-lsp/src/features/commands.rs) falls back to looking next to its
// own running executable when it's not on PATH — landing this at lsp/mdix,
// right beside lsp/mdix-lsp (see the matching MdixAddin.csproj and
// copy-binary.sh changes), means that fallback finds it with nothing else
// required inside VS4Mac either.
[assembly: ImportAddinFile("lsp/mdix")]

// 2026-08-06 — the two Resources/*.svg ImportAddinFile lines that used to be
// here are removed, along with the matching <StockIcon> extension and the
// icon="md-mdix-file" attribute in Manifest.addin.xml. The crash report from
// this exact build (mdix-vsmac/Issues.txt) shows VS4Mac's own crash monitor
// FailFasting on a native NSException thrown mid-redraw, right after this
// was the first build where the StockIcons extension point was actually
// correct (i.e. the first time the SVG was genuinely rasterized rather than
// silently skipped). Pulled out as a clean isolation test rather than a
// fourth guess at the SVG itself -- see the comment in Manifest.addin.xml
// for the reasoning and what to do with each outcome.

// "MonoDevelop.Ide" is still the correct host addin id under VS 2022 for Mac
// (17.x) — the Cocoa/net7 rewrite changed the UI shell and the addin build
// toolchain (see MdixAddin.csproj), but it kept the underlying Mono.Addins
// engine and the MonoDevelop.Ide addin id from the MonoDevelop/Xamarin
// Studio lineage. MonoDevelop.BuildInfo.Version is resolved at compile time
// via the Microsoft.VisualStudioMac.Sdk package reference now, instead of a
// manual HintPath into the app bundle.
//
// ROOT CAUSE OF "Could not resolve addin reference 'MonoDevelop.Core'/'MonoDevelop.Ide'":
// this bug predates my other fixes -- it was always in this file, it just
// never got the chance to surface because the build was failing earlier
// (wrong SDK, then the duplicate-item errors) before ever reaching addin
// dependency resolution. Per Mono.Addins' own rules, an unqualified
// dependency id is resolved *relative to your own addin's namespace* —
// since our namespace is "com.midmanstudio", plain "MonoDevelop.Ide" was
// being looked up as "com.midmanstudio.MonoDevelop.Ide", which obviously
// doesn't exist. The "::" prefix forces a global (unqualified) lookup.
// Source: mono/mono-addins wiki, "Dependencies of an add-in" — same exact
// pitfall demonstrated with a "TextEditor.Core" example.
//
// Declaring MonoDevelop.Core explicitly too, not just Ide, since the build
// error named both directly rather than just the one we'd declared -- Ide
// depends on Core internally, and I'd rather declare it than assume the SDK
// resolves it transitively.
[assembly: AddinDependency("::MonoDevelop.Core", MonoDevelop.BuildInfo.Version)]
[assembly: AddinDependency("::MonoDevelop.Ide", MonoDevelop.BuildInfo.Version)]
