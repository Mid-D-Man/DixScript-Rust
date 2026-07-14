using System;
using UnityEngine;

namespace MidManStudio.Mdix.Localization
{
    /// <summary>
    /// Baked representation of a locale .mdix file as a Unity ScriptableObject.
    ///
    /// In development builds:
    ///   Locale data loads through the FFI (MdixLocaleTable). No baking needed.
    ///   Hot-reload works because MdixDatabase watches the source file on disk.
    ///
    /// In shipped builds:
    ///   1. Open Window → MDIX → Localization Studio.
    ///   2. Select a locale MdixAsset and press "Bake".
    ///   3. Assign the resulting .asset to LocaleEntry.BakedAsset in the Inspector.
    ///   MdixLocalizationManager detects the populated reference and creates a
    ///   BakedLocaleTable — zero FFI calls, zero parsing, WebGL-safe.
    ///
    /// This SO can also be created via Assets → Create → MDIX → Localization →
    /// Locale Data Asset and populated manually for testing.
    /// </summary>
    [CreateAssetMenu(
        menuName = "MDIX/Localization/Locale Data Asset",
        fileName = "NewLocaleData",
        order    = 200)]
    public sealed class LocaleDataAsset : ScriptableObject
    {
        [Header("Identity")]
        [Tooltip("IETF language tag matching the @CONFIG locale value in the source .mdix.")]
        public string LocaleCode  = string.Empty;
        public string DisplayName = string.Empty;
        public string Bcp47       = string.Empty;

        [Header("Grammar rules")]
        [Tooltip("ONE_OTHER / ZERO_ONE_OTHER / SLAVIC / ARABIC / NONE — " +
                 "read from locale_plural_rule enum in the .mdix file.")]
        public string PluralRule   = "ONE_OTHER";
        [Tooltip("LTR or RTL.")]
        public string ScriptDir    = "LTR";
        [Tooltip("NONE / MASC_FEM / FULL.")]
        public string GenderSystem = "NONE";

        [Header("Number and date formatting")]
        public string DecimalSep   = ".";
        public string ThousandsSep = ",";
        public string DatePattern  = "MM/DD/YYYY";

        [Header("String entries")]
        [Tooltip("All flat key → value pairs. Covers simple strings and " +
                 "annotated key().value paths. Populated by the bake operation.")]
        public LocaleStringEntry[] Entries = Array.Empty<LocaleStringEntry>();

        [Header("Plural entries")]
        [Tooltip("Keys that expose named CLDR plural forms " +
                 "(one / other / few / many / zero / two). " +
                 "Built from p2/p4 quickfunc results and named dot-paths.")]
        public LocalePluralEntry[] PluralEntries = Array.Empty<LocalePluralEntry>();
    }

    // ── Serializable entry types ──────────────────────────────────────────────

    [Serializable]
    public struct LocaleStringEntry
    {
        [Tooltip("Full dotted key path, e.g. 'ui.play' or 'errors.save_failed'.")]
        public string Key;
        public string Value;
    }

    [Serializable]
    public struct LocalePluralEntry
    {
        [Tooltip("Base key, e.g. 'plural_enemies'. " +
                 "BakedLocaleTable looks up Key + CLDR form name at runtime.")]
        public string Key;

        [Tooltip("CLDR 'zero' form — e.g. 'No enemies' or 'нет врагов'.")]
        public string Zero;
        [Tooltip("CLDR 'one'  form — e.g. '1 enemy'   or '{0} враг'.")]
        public string One;
        [Tooltip("CLDR 'two'  form — Arabic only.")]
        public string Two;
        [Tooltip("CLDR 'few'  form — e.g. '{0} врага' (Slavic 2–4 tail).")]
        public string Few;
        [Tooltip("CLDR 'many' form — e.g. '{0} врагов' (Slavic 5+ or teen).")]
        public string Many;
        [Tooltip("CLDR 'other' form — English general plural, French singular, etc.")]
        public string Other;

        /// <summary>
        /// Returns the form matching the given CLDR form name.
        /// Falls through to Other when the specific form string is empty,
        /// so locales that don't distinguish a form still work correctly.
        /// </summary>
        public string GetForm(string formName)
        {
            var candidate = formName switch
            {
                "zero"  => Zero,
                "one"   => One,
                "two"   => Two,
                "few"   => Few,
                "many"  => Many,
                "other" => Other,
                _       => Other,
            };
            return string.IsNullOrEmpty(candidate) ? Other : candidate;
        }
    }
}
