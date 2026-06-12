using Mono.Addins;
using Mono.Addins.Description;

[assembly: Addin(
    "MdixLanguageSupport",
    Namespace = "com.midmanstudio",
    Version   = "1.0"
)]
[assembly: AddinName("DixScript (.mdix) Language Support")]
[assembly: AddinDescription("Syntax highlighting, completions, diagnostics and more for .mdix files via mdix-lsp.")]
[assembly: AddinAuthor("MidManStudio")]
[assembly: AddinUrl("https://github.com/Mid-D-Man/DixScript-Rust")]
