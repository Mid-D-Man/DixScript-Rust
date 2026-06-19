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

// VS for Mac's IDE shell kept the "MonoDevelop.Ide" addin id from its
// MonoDevelop/Xamarin Studio lineage all the way through retirement
// (17.6), so this is still the correct host dependency for a legacy install.
[assembly: AddinDependency("MonoDevelop.Ide", MonoDevelop.BuildInfo.Version)]
