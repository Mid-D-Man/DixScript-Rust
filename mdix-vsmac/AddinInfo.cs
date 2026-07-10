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
[assembly: AddinDependency("MonoDevelop.Ide", MonoDevelop.BuildInfo.Version)]
