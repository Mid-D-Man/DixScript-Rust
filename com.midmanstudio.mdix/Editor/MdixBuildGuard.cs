using System;
using System.Collections.Generic;
using System.IO;
using UnityEditor;
using UnityEditor.Build;
using UnityEditor.Build.Reporting;
using UnityEngine;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>
    /// Stops a build that would ship a secret.
    ///
    /// Fails the build when it would include
    ///   - a .mdix.key (the decryption key), or
    ///   - a plaintext .mdix whose @DLM encrypts it (its contents are exactly what the
    ///     encryption exists to hide; MdixAsset stores the whole source in the build).
    ///
    /// "Would include" means: referenced by an enabled scene, or sitting in a Resources folder
    /// or in StreamingAssets, followed through every dependency. Addressables groups and
    /// AssetBundles that you build yourself are not covered: keep keys and encrypted sources
    /// out of them.
    /// </summary>
    internal sealed class MdixBuildGuard : IPreprocessBuildWithReport
    {
        public int callbackOrder => -1000;

        public void OnPreprocessBuild(BuildReport report)
        {
            var problems = FindProblems();
            if (problems.Count == 0) return;

            throw new BuildFailedException(
                "MDIX build guard: this build would ship a secret.\n  " +
                string.Join("\n  ", problems) + "\n" +
                "Move these out of Resources and StreamingAssets and stop referencing them from scenes " +
                "and prefabs. Ship the .mdix.enc, provide the key from code with MdixKeys.Provide.");
        }

        [MenuItem("MidManStudio/MDIX Encryption/Check Build For Secrets")]
        private static void CheckNow()
        {
            var problems = FindProblems();
            if (problems.Count == 0)
            {
                Debug.Log("[MDIX] Build check: no keys or encrypted plaintext sources would be included.");
                return;
            }

            Debug.LogError(
                "[MDIX] Build check: a build right now would ship a secret.\n  " +
                string.Join("\n  ", problems));
        }

        internal static List<string> FindProblems()
        {
            var roots = new HashSet<string>(StringComparer.Ordinal);

            foreach (var scene in EditorBuildSettings.scenes)
            {
                if (scene.enabled && !string.IsNullOrEmpty(scene.path))
                    roots.Add(scene.path);
            }

            foreach (var path in AssetDatabase.GetAllAssetPaths())
            {
                var inAssets = path.StartsWith("Assets/", StringComparison.Ordinal);
                var inPkg    = path.StartsWith("Packages/", StringComparison.Ordinal);
                if (!inAssets && !inPkg) continue;

                var streaming = path.StartsWith("Assets/StreamingAssets/", StringComparison.OrdinalIgnoreCase);
                var resources = path.IndexOf("/Resources/", StringComparison.OrdinalIgnoreCase) >= 0;

                if ((streaming || resources) && !AssetDatabase.IsValidFolder(path))
                    roots.Add(path);
            }

            var rootArray = new string[roots.Count];
            roots.CopyTo(rootArray);

            var problems = new List<string>();
            var seen     = new HashSet<string>(StringComparer.Ordinal);

            foreach (var path in AssetDatabase.GetDependencies(rootArray, true))
            {
                if (!seen.Add(path)) continue;

                if (path.EndsWith(".mdix.key", StringComparison.OrdinalIgnoreCase))
                {
                    problems.Add($"{path}: a decryption key would be in the build.");
                }
                else if (path.EndsWith(".mdix", StringComparison.OrdinalIgnoreCase))
                {
                    string text;
                    try   { text = File.ReadAllText(path); }
                    catch { continue; }

                    if (MdixEncryption.DeclaresEncryption(text))
                        problems.Add($"{path}: the plaintext source of an encrypted file would be in the build.");
                }
            }

            return problems;
        }
    }
}
