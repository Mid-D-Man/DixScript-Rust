using System;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using System.Security.Cryptography;
using System.Text;
using UnityEditor;
using UnityEngine;
using MidManStudio.Mdix.Unity;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>
    /// Applies a shared "baked" icon to every [MdixBakeable] ScriptableObject
    /// type's MonoScript, so assets produced by MdixBakeWizard read as visually
    /// distinct from raw .mdix source assets (which use MdixImporter's own icon)
    /// at a glance in the Project window.
    ///
    /// Why per-type and not per-asset: EditorGUIUtility.SetIconForObject only
    /// officially supports GameObject/MonoScript targets. Pointing it at a
    /// ScriptableObject *instance* actually sets the icon on that instance's
    /// MonoScript under the hood, so the icon always ends up shared across
    /// every instance of the type regardless -- there is no supported way to
    /// color individual baked .asset files differently from one another.
    /// This applies the icon once, correctly, at the type level via
    /// MonoImporter.SetIcon, which is the officially documented path and is
    /// what actually persists across editor sessions.
    ///
    /// Runs once per distinct set of discovered [MdixBakeable] types (tracked
    /// via a hash in EditorPrefs) so it doesn't force a script reimport on
    /// every domain reload once the icons are already applied -- only when a
    /// new bakeable type shows up (or one is removed) does it run again.
    ///
    /// NOTE: written against documented Unity 2022.3 APIs (MonoImporter.SetIcon /
    /// GetIcon / SaveAndReimport, confirmed in the 2022.3 scripting reference).
    /// Not yet compiled or run inside the Editor -- please treat this as
    /// needing that first real pass before trusting it blindly.
    /// </summary>
    [InitializeOnLoad]
    internal static class MdixBakeableIconApplier
    {
        private const string BakedIconPath =
            "Packages/com.midmanstudio.mdix/Editor/Icons/mdix_icon_baked.png";

        private const string PrefKeyAppliedHash = "MdixStudio_BakeableIconsAppliedHash";

        static MdixBakeableIconApplier()
        {
            // Defer past the initial domain-load / asset-import burst so this
            // isn't competing with every other importer for AssetDatabase access.
            EditorApplication.delayCall += ApplyIfNeeded;
        }

        private static void ApplyIfNeeded()
        {
            var types = FindBakeableTypes();
            if (types.Count == 0) return;

            var hash = ComputeHash(types);
            if (EditorPrefs.GetString(PrefKeyAppliedHash, string.Empty) == hash)
                return; // Already applied to exactly this set of types.

            var icon = AssetDatabase.LoadAssetAtPath<Texture2D>(BakedIconPath);
            if (icon == null)
            {
                Debug.LogWarning(
                    $"MdixBakeableIconApplier: baked icon not found at '{BakedIconPath}'. " +
                    "Skipping icon assignment for [MdixBakeable] types.");
                return;
            }

            var appliedAny = false;

            foreach (var type in types)
            {
                var scriptPath = FindScriptPathForType(type);
                if (string.IsNullOrEmpty(scriptPath)) continue;

                if (!(AssetImporter.GetAtPath(scriptPath) is MonoImporter importer))
                    continue;

                // Skip if it already has exactly this icon assigned.
                if (importer.GetIcon() == icon) continue;

                importer.SetIcon(icon);
                importer.SaveAndReimport();
                appliedAny = true;
            }

            EditorPrefs.SetString(PrefKeyAppliedHash, hash);

            if (appliedAny)
                Debug.Log("MdixBakeableIconApplier: applied the baked-data icon to [MdixBakeable] types.");
        }

        // Mirrors MdixBakeWizard.RefreshTypes()'s assembly filtering exactly,
        // so the two stay in agreement about what counts as a bakeable type.
        private static List<Type> FindBakeableTypes()
        {
            var results = new List<Type>();

            foreach (var assembly in AppDomain.CurrentDomain.GetAssemblies())
            {
                var name = assembly.GetName().Name ?? string.Empty;
                if (name.StartsWith("Unity", StringComparison.Ordinal))    continue;
                if (name.StartsWith("System", StringComparison.Ordinal))   continue;
                if (name.StartsWith("mscorlib", StringComparison.Ordinal)) continue;
                if (name.StartsWith("Mono.", StringComparison.Ordinal))    continue;

                Type[] types;
                try   { types = assembly.GetTypes(); }
                catch { continue; }

                foreach (var type in types)
                {
                    if (!type.IsClass || type.IsAbstract) continue;
                    if (!typeof(ScriptableObject).IsAssignableFrom(type)) continue;
                    if (type.GetCustomAttribute<MdixBakeableAttribute>() == null) continue;

                    results.Add(type);
                }
            }

            return results;
        }

        // No TypeCache-based script->type lookup ships in the public API, so
        // this walks every MonoScript in the project once per new type-set.
        // Only runs when the bakeable-type hash changes (see ApplyIfNeeded),
        // not on every domain reload.
        private static string FindScriptPathForType(Type type)
        {
            foreach (var guid in AssetDatabase.FindAssets("t:MonoScript"))
            {
                var path   = AssetDatabase.GUIDToAssetPath(guid);
                var script = AssetDatabase.LoadAssetAtPath<MonoScript>(path);
                if (script != null && script.GetClass() == type)
                    return path;
            }
            return null;
        }

        private static string ComputeHash(List<Type> types)
        {
            var joined = string.Join("|", types
                .Select(t => t.FullName)
                .OrderBy(n => n, StringComparer.Ordinal));

            using var sha   = SHA256.Create();
            var       bytes = sha.ComputeHash(Encoding.UTF8.GetBytes(joined));
            var       sb    = new StringBuilder(bytes.Length * 2);
            foreach (var b in bytes) sb.Append(b.ToString("x2"));
            return sb.ToString();
        }
    }
}
