// com.midmanstudio.mdix.localization/Runtime/MdixLocaleTable.cs
using System;
using MidManStudio.Mdix.Core;
using MidManStudio.Mdix.Unity;
using UnityEngine;

namespace MidManStudio.Mdix.Localization
{
    /// <summary>
    /// Wraps a loaded <see cref="MdixDatabase"/> representing one locale.
    /// Provides typed lookup with fallback to a default locale table.
    /// </summary>
    public sealed class MdixLocaleTable : IDisposable
    {
        private MdixDatabase? _db;
        private MdixLocaleTable? _fallback;

        public string LocaleCode { get; }
        public string DisplayName { get; }
        public bool IsLoaded => _db != null && _db.IsValid;

        public MdixLocaleTable(string localeCode, MdixDatabase db, MdixLocaleTable? fallback = null)
        {
            LocaleCode  = localeCode;
            _db         = db;
            _fallback   = fallback;
            DisplayName = db.GetString("locale_display_name").UnwrapOr(localeCode);
        }

        /// <summary>
        /// Get a localized string by key.
        /// Falls back to the fallback locale if the key is absent.
        /// Returns the key itself if not found in either.
        /// </summary>
        public string Get(string key)
        {
            if (_db != null && _db.IsValid)
            {
                var result = _db.GetString(key);
                if (result.IsSuccess) return result.SuccessResult;
            }

            if (_fallback != null)
                return _fallback.Get(key);

            return key;
        }

        /// <summary>
        /// Get a localized string and apply String.Format substitutions.
        /// Example: Get("gameplay.score", 1250) → "Score: 1250"
        /// </summary>
        public string Get(string key, params object[] args)
        {
            var template = Get(key);
            try   { return args.Length > 0 ? string.Format(template, args) : template; }
            catch { return template; }
        }

        /// <summary>
        /// Get a plural form based on count.
        /// Plural arrays are stored as DixScript group arrays:
        ///   plural_enemies:: "No enemies", "1 enemy", "{0} enemies"
        /// Selects index based on English-style pluralization (0=zero, 1=one, 2+=other).
        /// Override <see cref="GetPluralIndex"/> for language-specific rules.
        /// </summary>
        public string GetPlural(string key, int count)
        {
            if (_db == null || !_db.IsValid)
                return _fallback?.GetPlural(key, count) ?? key;

            var len = _db.GetArrayLength(key).UnwrapOr(0);
            if (len == 0)
                return _fallback?.GetPlural(key, count) ?? key;

            int index = GetPluralIndex(count, len);
            var itemPath = $"{key}[{index}]";
            var template = _db.GetString(itemPath).UnwrapOr(key);

            try   { return string.Format(template, count); }
            catch { return template; }
        }

        /// <summary>
        /// Returns the plural array index for a given count.
        /// Override this in a subclass for non-English plural rules.
        /// Default: English (0=zero, 1=one, 2+=other).
        /// </summary>
        protected virtual int GetPluralIndex(int count, int arrayLength)
        {
            if (arrayLength == 1) return 0;
            if (arrayLength == 2) return count == 1 ? 1 : 0;
            // 3+ form arrays: 0=zero, 1=one, 2=other
            return count == 0 ? 0 : count == 1 ? 1 : Math.Min(2, arrayLength - 1);
        }

        public bool HasKey(string key) =>
            (_db?.Exists(key) ?? false) || (_fallback?.HasKey(key) ?? false);

        public void Dispose()
        {
            _db?.Dispose();
            _db = null;
        }
    }
}
