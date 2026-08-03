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
// The missing icon is almost certainly what corrupted the Solution/file pad:
// Manifest.addin.xml registers a StockIcon for every .mdix file, and VS4Mac
// has to resolve that icon the moment it renders a tree node for one — i.e.
// exactly when you expand a folder containing one, or immediately if it's at
// the root. With the resource never actually shipped, that resolution fails
// right in the middle of tree rendering.
//
// Paths below are relative to the addin's own install directory, matching
// where MdixAddin.csproj's CopyToOutputDirectory items actually place these
// files at build time (bin/<config>/net7.0/lsp/mdix-lsp,
// bin/<config>/net7.0/Resources/*.svg).
// ----------------------------------------------------------------------------
[assembly: ImportAddinFile("lsp/mdix-lsp")]
[assembly: ImportAddinFile("Resources/mdix-file.svg")]
[assembly: ImportAddinFile("Resources/mdix-file-dark.svg")]

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
