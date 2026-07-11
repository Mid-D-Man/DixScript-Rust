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
