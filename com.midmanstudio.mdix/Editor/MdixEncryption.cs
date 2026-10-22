using System;
using System.IO;
using System.Text;
using UnityEditor;
using UnityEngine;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>
    /// Editor-side handling of encrypted MDIX files: producing the .mdix.enc and .mdix.key from a
    /// .mdix source, and keeping the key out of git.
    ///
    /// How the files are produced: loading a .mdix FILE (not text) runs its @DLM pipeline and
    /// writes the outputs beside the source — the same step the `mdix compile` command performs.
    /// Every run generates a NEW random key, so the .enc and the .key always come as a pair:
    /// recompile = redeploy both.
    /// </summary>
    internal static class MdixEncryption
    {
        /// <summary>Patterns added to .gitignore. The .meta files go with the files they describe.</summary>
        private static readonly string[] KeyIgnorePatterns = { "*.mdix.key", "*.mdix.key.meta" };

        internal sealed class Outcome
        {
            public bool   Success;
            public bool   Generated;
            public string Message       = string.Empty;
            public string EncPath       = string.Empty;
            public string KeyPath       = string.Empty;
            public string GitIgnoreNote = string.Empty;
        }

        // ── Detection ─────────────────────────────────────────────────────────

        /// <summary>True when the source has an @DLM section that names a DEncryptor.</summary>
        internal static bool DeclaresEncryption(string source) =>
            !string.IsNullOrEmpty(source)
            && source.Contains("@DLM")
            && source.Contains("DEncryptor");

        /// <summary>True when the source has an @DLM section that names a DEncryptor or DCompressor.</summary>
        internal static bool DeclaresDlmOutput(string source) =>
            !string.IsNullOrEmpty(source)
            && source.Contains("@DLM")
            && (source.Contains("DEncryptor") || source.Contains("DCompressor"));

        // ── Generate ──────────────────────────────────────────────────────────

        /// <summary>
        /// Compile the .mdix file at <paramref name="assetPath"/> from disk and write its
        /// .mdix.enc and .mdix.key beside it. The file must already be saved.
        /// </summary>
        internal static Outcome Generate(string assetPath)
        {
            var o = new Outcome();

            if (string.IsNullOrEmpty(assetPath) || !File.Exists(assetPath))
            {
                o.Message = "Save the file first: the .enc is built from the file on disk.";
                return o;
            }

            string source;
            try   { source = File.ReadAllText(assetPath); }
            catch (Exception ex)
            {
                o.Message = $"Cannot read {Path.GetFileName(assetPath)}: {ex.Message}";
                return o;
            }

            if (!DeclaresDlmOutput(source))
            {
                o.Success = true;
                o.Message = "Compiles. Nothing to encrypt: there is no @DLM section with DEncryptor or DCompressor.";
                return o;
            }

            var dir  = Path.GetDirectoryName(assetPath) ?? string.Empty;
            var stem = Path.GetFileNameWithoutExtension(assetPath);
            o.EncPath = Path.Combine(dir, stem + ".mdix.enc").Replace('\\', '/');
            o.KeyPath = Path.Combine(dir, stem + ".mdix.key").Replace('\\', '/');

            // File timestamps are coarse on some file systems; allow a little slack.
            var startedUtc = DateTime.UtcNow.AddSeconds(-2);

            var result = MidManStudio.Mdix.Dix.Load(Path.GetFullPath(assetPath));
            if (result.IsFailure)
            {
                var message = result.Error.Message;
                if (message.IndexOf("password", StringComparison.OrdinalIgnoreCase) >= 0)
                    message += "  (Password-mode files cannot be compiled from Unity yet: compile them with" +
                               " the mdix command line tool, then load them with a password.)";
                o.Message = message;
                return o;
            }
            result.SuccessResult.Dispose();

            var encWritten = File.Exists(o.EncPath) && File.GetLastWriteTimeUtc(o.EncPath) >= startedUtc;
            var keyWritten = File.Exists(o.KeyPath) && File.GetLastWriteTimeUtc(o.KeyPath) >= startedUtc;

            if (!encWritten)
            {
                o.Message = "Compiled, but no .mdix.enc was written. Check that @DLM lists DCompressor or DEncryptor.";
                return o;
            }

            if (keyWritten)
                o.GitIgnoreNote = EnsureKeyIgnored();

            // Bring the new files into the Project window so their importers (and icons) run.
            if (assetPath.StartsWith("Assets/", StringComparison.Ordinal))
            {
                AssetDatabase.ImportAsset(o.EncPath, ImportAssetOptions.ForceUpdate);
                if (keyWritten)
                    AssetDatabase.ImportAsset(o.KeyPath, ImportAssetOptions.ForceUpdate);
            }

            o.Success   = true;
            o.Generated = true;
            o.Message   = keyWritten
                ? $"Encrypted → {stem}.mdix.enc + {stem}.mdix.key (new key; the key is gitignored and never ships)"
                : $"Written → {stem}.mdix.enc";
            return o;
        }

        // ── Git guardrail ─────────────────────────────────────────────────────

        /// <summary>
        /// Make sure the project's .gitignore ignores .mdix.key files. Adds the missing lines to an
        /// existing .gitignore, or creates one when the project is a git repository. Returns a note
        /// for the person (empty when nothing needed doing).
        /// </summary>
        internal static string EnsureKeyIgnored()
        {
            try
            {
                var root       = Directory.GetParent(Application.dataPath).FullName;
                var ignorePath = Path.Combine(root, ".gitignore");
                var gitPath    = Path.Combine(root, ".git");
                var inRepo     = Directory.Exists(gitPath) || File.Exists(gitPath);

                if (!File.Exists(ignorePath) && !inRepo)
                    return "No .gitignore found. If you use git, ignore *.mdix.key yourself.";

                var existing = File.Exists(ignorePath) ? File.ReadAllText(ignorePath) : string.Empty;
                var present  = existing.Split('\n');

                var sb      = new StringBuilder(existing);
                var nl      = existing.Contains("\r\n") ? "\r\n" : "\n";
                var added   = 0;

                foreach (var pattern in KeyIgnorePatterns)
                {
                    var found = false;
                    foreach (var raw in present)
                    {
                        var line = raw.Trim();
                        if (line == pattern || line == "**/" + pattern || line == "/" + pattern)
                        {
                            found = true;
                            break;
                        }
                    }

                    if (found) continue;

                    if (added == 0)
                    {
                        if (sb.Length > 0 && !existing.EndsWith("\n", StringComparison.Ordinal))
                            sb.Append(nl);
                        sb.Append(nl).Append("# MDIX encryption keys: never commit (added by MDIX Studio)").Append(nl);
                    }

                    sb.Append(pattern).Append(nl);
                    added++;
                }

                if (added == 0) return string.Empty;

                File.WriteAllText(ignorePath, sb.ToString());
                return "Added *.mdix.key to .gitignore.";
            }
            catch (Exception ex)
            {
                return $"Could not update .gitignore ({ex.Message}). Ignore *.mdix.key yourself.";
            }
        }
    }

    /// <summary>The tinted icons for the encrypted-file types.</summary>
    internal static class MdixIcons
    {
        private const string Dir = "Packages/com.midmanstudio.mdix/Editor/Icons/";

        internal static Texture2D Enc => AssetDatabase.LoadAssetAtPath<Texture2D>(Dir + "mdix_icon_enc.png");
        internal static Texture2D Key => AssetDatabase.LoadAssetAtPath<Texture2D>(Dir + "mdix_icon_key.png");
    }
}
