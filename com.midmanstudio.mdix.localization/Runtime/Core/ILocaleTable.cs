// com.midmanstudio.mdix.localization/Runtime/Core/ILocaleTable.cs
using System;

namespace MidManStudio.Mdix.Localization
{
    /// <summary>
    /// Contract for all locale table implementations.
    /// MdixLocalizationManager holds ILocaleTable — never the concrete type —
    /// so the live FFI path (MdixLocaleTable) and the baked SO path
    /// (BakedLocaleTable) are interchangeable at runtime with no manager changes.
    /// </summary>
    public interface ILocaleTable : IDisposable
    {
        /// <summary>IETF language tag, e.g. "en_US".</summary>
        string LocaleCode { get; }

        /// <summary>Display name in the locale's own script, e.g. "Français (France)".</summary>
        string DisplayName { get; }

        /// <summary>True when the underlying data is available and valid.</summary>
        bool IsLoaded { get; }

        /// <summary>
        /// Grammar and formatting metadata read from locale_* keys.
        /// Populated once on construction — zero overhead per Get call.
        /// </summary>
        MdixLocaleMetadata Metadata { get; }

        /// <summary>
        /// Get a localized string by key path.
        /// Returns the key itself if not found in this table or its fallback.
        /// </summary>
        string Get(string key);

        /// <summary>
        /// Get a localized string with String.Format substitutions.
        /// Example: Get("gameplay.score", 1250) → "Score: 1250"
        /// </summary>
        string Get(string key, params object[] args);

        /// <summary>
        /// Get a plural-aware string for the given count.
        ///
        /// Resolution order:
        ///   1. count == 0 and a "zero" named form exists → use it directly.
        ///   2. Named form: key.{formName} where formName comes from
        ///      MdixPluralResolver.GetFormName(Metadata.PluralRule, count).
        ///   3. Indexed array fallback: key[n] for plain :: group arrays.
        ///   4. Fallback locale table.
        ///   5. The key string itself.
        ///
        /// The resolved template is formatted with String.Format(template, count).
        /// </summary>
        string GetPlural(string key, int count);

        /// <summary>Returns true if the key exists in this table or its fallback.</summary>
        bool HasKey(string key);
    }
}
