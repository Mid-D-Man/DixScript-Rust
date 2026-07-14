using System;
using System.Collections.Generic;

namespace MidManStudio.Mdix.Localization
{
    /// <summary>
    /// ILocaleTable backed by a LocaleDataAsset ScriptableObject.
    /// Active when LocaleEntry.BakedAsset is assigned in the Inspector.
    ///
    /// All lookups are pure C# Dictionary operations — zero P/Invoke,
    /// zero parsing, WebGL-safe, IL2CPP strip-safe. The dictionaries are
    /// built once in the constructor; all subsequent calls are O(1).
    ///
    /// Zero-form special case: when count == 0 and the plural entry has a
    /// non-empty Zero field, the Zero form is returned directly without
    /// consulting MdixPluralResolver. This lets Slavic/Arabic locales
    /// author an explicit "No enemies" / "нет врагов" form even though
    /// the CLDR rule for those families technically maps 0 to "many".
    /// </summary>
    public sealed class BakedLocaleTable : ILocaleTable
    {
        private readonly Dictionary<string, string>            _strings;
        private readonly Dictionary<string, LocalePluralEntry> _plurals;
        private readonly ILocaleTable?                         _fallback;

        public string             LocaleCode  { get; }
        public string             DisplayName { get; }
        public bool               IsLoaded    => true;
        public MdixLocaleMetadata Metadata    { get; }

        public BakedLocaleTable(LocaleDataAsset asset, ILocaleTable? fallback = null)
        {
            if (asset == null) throw new ArgumentNullException(nameof(asset));

            _fallback   = fallback;
            LocaleCode  = asset.LocaleCode;
            DisplayName = asset.DisplayName;

            Metadata = new MdixLocaleMetadata(
                bcp47:        asset.Bcp47,
                displayName:  asset.DisplayName,
                pluralRule:   asset.PluralRule,
                scriptDir:    asset.ScriptDir,
                genderSystem: asset.GenderSystem,
                decimalSep:   asset.DecimalSep,
                thousandsSep: asset.ThousandsSep,
                datePattern:  asset.DatePattern);

            _strings = new Dictionary<string, string>(
                asset.Entries.Length, StringComparer.Ordinal);

            foreach (var entry in asset.Entries)
            {
                if (!string.IsNullOrEmpty(entry.Key))
                    _strings[entry.Key] = entry.Value ?? string.Empty;
            }

            _plurals = new Dictionary<string, LocalePluralEntry>(
                asset.PluralEntries.Length, StringComparer.Ordinal);

            foreach (var entry in asset.PluralEntries)
            {
                if (!string.IsNullOrEmpty(entry.Key))
                    _plurals[entry.Key] = entry;
            }
        }

        public string Get(string key)
        {
            if (_strings.TryGetValue(key, out var value))
                return value;

            if (_fallback != null)
                return _fallback.Get(key);

            return key;
        }

        public string Get(string key, params object[] args)
        {
            var template = Get(key);
            try   { return args.Length > 0 ? string.Format(template, args) : template; }
            catch { return template; }
        }

        public string GetPlural(string key, int count)
        {
            if (_plurals.TryGetValue(key, out var entry))
            {
                string formName;

                // Pre-empt the resolver: if count == 0 and the locale provides an
                // explicit zero form, use it. Handles Slavic "нет врагов" correctly
                // even though CLDR maps 0 to "many" for that rule family.
                if (count == 0 && !string.IsNullOrEmpty(entry.Zero))
                    formName = "zero";
                else
                    formName = MdixPluralResolver.GetFormName(Metadata.PluralRule, count);

                var template = entry.GetForm(formName);
                try   { return string.Format(template, count); }
                catch { return template; }
            }

            if (_fallback != null)
                return _fallback.GetPlural(key, count);

            return key;
        }

        public bool HasKey(string key) =>
            _strings.ContainsKey(key)  ||
            _plurals.ContainsKey(key)  ||
            (_fallback?.HasKey(key) ?? false);

        public void Dispose()
        {
            // No unmanaged resources owned by this table.
            // The manager owns both the active and fallback table references
            // and is responsible for disposing them on locale switch or destroy.
        }
    }
}
