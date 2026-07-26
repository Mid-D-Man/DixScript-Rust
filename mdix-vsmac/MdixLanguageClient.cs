// ============================================================================
// Replaces MdixLanguageClientProvider.cs (delete that file when adding this
// one — see Issues.cs for the full original error dump, and
// MdixLanguageClient.cs.experimental's own header comment for how this was
// tracked down).
//
// ROOT CAUSE, confirmed: MdixLanguageClientProvider.cs implemented against
// `MonoDevelop.Ide.Lsp.LspLanguageClientProvider` — a base class that never
// existed in any real MonoDevelop.Ide or VS4Mac SDK release. The `[Extension]`
// / `protected override` shape it used doesn't correspond to anything real.
//
// The REAL mechanism, confirmed via two independent sources:
//   1. Matt Ward's (mrward, the actual VS-for-Mac LSP integration author)
//      own walkthrough, "Language Server Protocol support in Visual Studio
//      for Mac 7.4" (lastexitcode.com, 2018) — states outright: "The API
//      provided by the Language Server Client extension follows the API
//      defined by the Language Server Protocol extension used by Visual
//      Studio on Windows as closely as possible." I.e. VS4Mac's
//      `ILanguageClient` is not a parallel, VS4Mac-specific interface — it's
//      registered via ordinary MEF ([Export], System.ComponentModel.
//      Composition), exactly like the Windows one.
//   2. Your own project's build cache (.vs/MdixAddin/xs/project-cache/
//      MdixAddin-Debug.json) lists
//      /Applications/Visual Studio.app/Contents/MonoBundle/
//      Microsoft.VisualStudio.LanguageServer.Client.dll as an ACTUAL
//      resolved reference on your machine — this closes the exact
//      uncertainty the .experimental file's header raised ("I don't have a
//      verified source for that assembly... it may not exist in a working
//      form at all"). It does; the 2022 Cocoa/.NET7 rewrite bundles it
//      directly rather than requiring the old separate mrward .mpack.
//
// What's DIFFERENT here vs. MdixLanguageClient.cs.experimental (which was on
// the right track structurally, just against an incomplete/older picture of
// the interface):
//   - Added `ShowNotificationOnInitializeFailed` (bool) — a property that
//     doesn't exist in Matt Ward's 2018 walkthrough OR the .experimental
//     file, but is required by the interface as it exists in the
//     Microsoft.VisualStudio.LanguageServer.Client package version actually
//     resolved on this machine (confirmed via the official current API
//     reference: learn.microsoft.com/en-us/dotnet/api/microsoft.
//     visualstudio.languageserver.client.ilanguageclient).
//   - `OnServerInitializeFailedAsync` — the interface declares TWO
//     overloads, `(Exception)` and `(ILanguageClientInitializationInfo)`.
//     The .experimental file only implemented the Exception one; your build
//     error named the ILanguageClientInitializationInfo one specifically as
//     missing. Both are implemented below.
//   - Manifest.addin.xml needs a matching update (delivered alongside this
//     file) — without registering this assembly for MEF composition
//     scanning under "/MonoDevelop/Ide/Composition", the host will never
//     discover the [Export(typeof(ILanguageClient))] class below no matter
//     how correctly it compiles.
//
// I can't compile this myself — no .NET toolchain, and even with one, no
// copy of the actual VS4Mac SDK/app bundle to build against. This is a
// best-effort synthesis against the official, current interface reference
// plus your own build cache's confirmed assembly resolution — it should be
// structurally correct, but treat the next `vstool build`/`dotnet build` run
// on your machine as the real test, and send me whatever it says if
// anything's still off.
// ============================================================================

using Microsoft.VisualStudio.LanguageServer.Client;
using Microsoft.VisualStudio.Threading;
using Microsoft.VisualStudio.Utilities;
using StreamJsonRpc;
using System;
using System.Collections.Generic;
using System.ComponentModel.Composition;
using System.Diagnostics;
using System.IO;
using System.Reflection;
using System.Threading;
using System.Threading.Tasks;

namespace MidManStudio.Mdix
{
    // Maps the .mdix file extension to a content type, so the IDE knows this
    // language client applies to DixScript files. Required companion piece
    // alongside the client itself -- without it ActivateAsync never fires.
    public static class MdixContentDefinition
    {
#pragma warning disable CS0649 // assigned by MEF at composition time, not by us
        [Export]
        [Name("mdix")]
        [BaseDefinition(CodeRemoteContentDefinition.CodeRemoteContentTypeName)]
        internal static ContentTypeDefinition? MdixContentTypeDefinition;

        [Export]
        [FileExtension(".mdix")]
        [ContentType("mdix")]
        internal static FileExtensionToContentTypeDefinition? MdixFileExtensionDefinition;
#pragma warning restore CS0649
    }

    [ContentType("mdix")]
    [Export(typeof(ILanguageClient))]
    public class MdixLanguageClient : ILanguageClient
    {
        public event AsyncEventHandler<EventArgs>? StartAsync;

#pragma warning disable CS0067 // required by ILanguageClient, unused here
        public event AsyncEventHandler<EventArgs>? StopAsync;
#pragma warning restore CS0067

        public string Name => "DixScript Language Client";

        public IEnumerable<string> ConfigurationSections
        {
            get { yield return "mdix"; }
        }

        public object? InitializationOptions => null;
        public IEnumerable<string>? FilesToWatch => null;
        public object? MiddleLayer => null;
        public object? CustomMessageTarget => null;

        // If the server fails to initialize, surface it to the user rather
        // than failing silently -- for a fresh install where MDIX_LSP_PATH
        // isn't set and the bundled binary isn't there yet, this is the only
        // signal they'd otherwise get.
        public bool ShowNotificationOnInitializeFailed => true;

        public Task<Connection?> ActivateAsync(CancellationToken token)
        {
            var serverPath = ResolveServerPath();

            var info = new ProcessStartInfo
            {
                FileName               = serverPath,
                UseShellExecute        = false,
                RedirectStandardInput  = true,
                RedirectStandardOutput = true,
                CreateNoWindow         = true,
            };

            var process = new Process { StartInfo = info };

            Connection? connection = null;
            if (process.Start())
            {
                connection = new Connection(
                    process.StandardOutput.BaseStream,
                    process.StandardInput.BaseStream);
            }

            return Task.FromResult(connection);
        }

        public async Task OnLoadedAsync()
        {
            if (StartAsync != null)
                await StartAsync.InvokeAsync(this, EventArgs.Empty);
        }

        public Task OnServerInitializedAsync() => Task.CompletedTask;

        public Task OnServerInitializeFailedAsync(Exception e) => Task.CompletedTask;

        public Task OnServerInitializeFailedAsync(ILanguageClientInitializationInfo initializationState)
            => Task.CompletedTask;

        // Same resolution order as the VS Code / mdix-lsp wrappers: env
        // override, then bundled binary next to this assembly, then PATH.
        static string ResolveServerPath()
        {
            var env = Environment.GetEnvironmentVariable("MDIX_LSP_PATH");
            if (!string.IsNullOrEmpty(env) && File.Exists(env))
                return env;

            var addinDir = Path.GetDirectoryName(Assembly.GetExecutingAssembly().Location)!;
            var bundled  = Path.Combine(addinDir, "lsp", "mdix-lsp");
            if (File.Exists(bundled))
                return bundled;

            return "mdix-lsp"; // fall back to PATH resolution by the shell
        }
    }
}
