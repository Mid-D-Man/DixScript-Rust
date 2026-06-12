using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using Mono.Addins;
using MonoDevelop.Ide.Lsp;
using MonoDevelop.Core;

namespace MidManStudio.Mdix
{
    [Extension]
    public class MdixLanguageClientProvider : LspLanguageClientProvider
    {
        // File extensions this server handles.
        protected override IEnumerable<string> SupportedFileExtensions
            => new[] { ".mdix" };

        protected override string LanguageId => "mdix";

        protected override string GetServerPath()
        {
            // 1. Env override (useful during development)
            var env = Environment.GetEnvironmentVariable("MDIX_LSP_PATH");
            if (!string.IsNullOrEmpty(env) && File.Exists(env))
                return env;

            // 2. Bundled binary next to the addin assembly
            var addinDir  = Path.GetDirectoryName(Assembly.GetExecutingAssembly().Location)!;
            var platform  = GetPlatformDir();
            var binName   = "mdix-lsp";
            var bundled   = Path.Combine(addinDir, "bin", platform, binName);
            if (File.Exists(bundled))
                return bundled;

            // 3. System PATH
            var fromPath = FindOnPath(binName);
            if (fromPath != null)
                return fromPath;

            throw new InvalidOperationException(
                "mdix-lsp binary not found. " +
                "Set MDIX_LSP_PATH or place the binary in the addin's bin/ directory."
            );
        }

        // ── Helpers ──────────────────────────────────────────────────────────

        static string GetPlatformDir()
        {
            var os   = Environment.OSVersion.Platform;
            var arch = System.Runtime.InteropServices.RuntimeInformation.ProcessArchitecture;
            return (os, arch) switch
            {
                (PlatformID.Unix, System.Runtime.InteropServices.Architecture.Arm64) => "darwin-arm64",
                (PlatformID.Unix, System.Runtime.InteropServices.Architecture.X64)   => "darwin-x64",
                (PlatformID.Win32NT, _)                                               => "win32-x64",
                _                                                                     => "linux-x64",
            };
        }

        static string? FindOnPath(string name)
        {
            try
            {
                var result = new System.Diagnostics.Process
                {
                    StartInfo = new System.Diagnostics.ProcessStartInfo
                    {
                        FileName  = "which",
                        Arguments = name,
                        RedirectStandardOutput = true,
                        UseShellExecute = false,
                    }
                };
                result.Start();
                var line = result.StandardOutput.ReadLine()?.Trim();
                result.WaitForExit();
                return (line != null && File.Exists(line)) ? line : null;
            }
            catch { return null; }
        }
    }
}
