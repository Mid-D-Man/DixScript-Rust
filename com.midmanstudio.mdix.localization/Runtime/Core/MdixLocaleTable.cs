using System;
using System.Collections.Generic;
using MidManStudio.Mdix.Core;

namespace MidManStudio.Mdix.Localization
{
    /// <summary>
    /// Produced by MdixLocaleTable.GetValidationIssues(). Reports keys where
    /// the key() @QUICKFUNCS helper detected a character limit violation at
    /// parse time (valid = false baked into the locale data).
    /// Only available from live tables — baked tables do not carry this at runtime.
    /// </summary>
    public readonly struct MdixLocaleValidationIssue
    {
        /// <summary>Base key of the annotated entry, e.g. "ui_new_game".</summary>
        public string Key { get; }

        /// <summary>
        /// Warning string produced by the key() quickfunc,
        /// e.g. "OVER: 18 > 16 chars".
        /// </summary>
        public string Warning { get; }

        public MdixLocaleValidationIssue(string key, string warning)
        {
            Key     = key;
            Warning = warning;
        }

        public override string ToString() => $"[{Key}] {Warning}";
    }

    /// <summary>
    /// Live FFI-backed locale table. Wraps a loaded MdixDatabase and implements
    /// ILocaleTable so MdixLocalizationManager can swap it out for a BakedLocaleTable
    /// in shipped builds without any manager changes.
    ///
    /// Plural resolution order:
    ///   1. count == 0 + explicit .zero path — lets Slavic/Arabic locales
    ///      provide "нет врагов" even though CLDR maps 0 to "many" for those rules.
    ///   2. Named dot-path: key.{formName} produced by p2/p4 @QUICKFUNCS helpers.
    ///   3. Indexed array fallback: key[n] for plain :: group arrays (backward compat).
    ///   4. Fallback ILocaleTable.
    ///   5. The key string itself.
    /// </summary>
    public sealed class MdixLocaleTable : ILocaleTable
    {
        private MdixDatabase?  _db;
        private ILocaleTable?  _fallback;

        public string             LocaleCode  { get; }
        public string             DisplayName => Metadata.DisplayName;
        public bool               IsLoaded    => _db != null && _db.IsValid;
        public MdixLocaleMetadata Metadata    { get; }

        public MdixLocaleTable(
            string        localeCode,
            MdixDatabase  db,
            ILocaleTable? fallback = null)
        {
            LocaleCode = localeCode;
            _db        = db ?? throw new ArgumentNullException(nameof(db));
            _fallback  = fallback;
            Metadata   = BuildMetadata(db, localeCode);
        }

        // ── ILocaleTable ──────────────────────────────────────────────────────

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

        public string Get(string key, params object[] args)
        {
            var template = Get(key);
            try   { return args.Length > 0 ? string.Format(template, args) : template; }
            catch { return template; }
        }

        public string GetPlural(string key, int count)
        {
            if (_db == null || !_db.IsValid)
                return _fallback?.GetPlural(key, count) ?? key;

            // ── 1. Explicit zero form ─────────────────────────────────────────
            // Pre-empt the rule resolver when count == 0 and the locale provides
            // an explicit zero path (e.g. plural_enemies.zero = "нет врагов").
            // This handles Slavic/Arabic locales whose CLDR rule maps 0 to "many".
            if (count == 0)
            {
                var zeroPath   = $"{key}.zero";
                var zeroResult = _db.GetString(zeroPath);
                if (zeroResult.IsSuccess)
                {
                    try   { return string.Format(zeroResult.SuccessResult, count); }
                    catch { return zeroResult.SuccessResult; }
                }
            }

            // ── 2. Named dot-path form (p2/p4 output) ────────────────────────
            var formName  = MdixPluralResolver.GetFormName(Metadata.PluralRule, count);
            var namedPath = $"{key}.{formName}";
            var named     = _db.GetString(namedPath);
            if (named.IsSuccess)
            {
                try   { return string.Format(named.SuccessResult, count); }
                catch { return named.SuccessResult; }
            }

            // ── 3. Indexed array fallback (plain :: group arrays) ─────────────
            var arrayLen = _db.GetArrayLength(key).UnwrapOr(0);
            if (arrayLen > 0)
            {
                var index    = LegacyArrayIndex(count, arrayLen);
                var template = _db.GetString($"{key}[{index}]").UnwrapOr(key);
                try   { return string.Format(template, count); }
                catch { return template; }
            }

            // ── 4. Fallback locale ────────────────────────────────────────────
            if (_fallback != null)
                return _fallback.GetPlural(key, count);

            return key;
        }

        public bool HasKey(string key) =>
            (_db?.Exists(key) ?? false) || (_fallback?.HasKey(key) ?? false);

        // ── Validation ────────────────────────────────────────────────────────

        /// <summary>
        /// Scans all keys in the active database for entries where the key()
        /// @QUICKFUNCS helper baked valid = false (value exceeded max_chars).
        /// Returns an empty list when the database is not loaded.
        /// </summary>
        public IReadOnlyList<MdixLocaleValidationIssue> GetValidationIssues()
        {
            if (_db == null || !_db.IsValid)
                return Array.Empty<MdixLocaleValidationIssue>();

            var issues  = new List<MdixLocaleValidationIssue>();
            var allKeys = _db.GetKeys().UnwrapOr(Array.Empty<string>());

            const string ValidSuffix = ".valid";

            foreach (var k in allKeys)
            {
                if (!k.EndsWith(ValidSuffix, StringComparison.Ordinal)) continue;

                var validResult = _db.GetBool(k);
                if (!validResult.IsSuccess || validResult.SuccessResult) continue;

                var baseKey = k.Substring(0, k.Length - ValidSuffix.Length);
                var warning = _db.GetString($"{baseKey}.warning")
                                 .UnwrapOr("Character limit exceeded");

                issues.Add(new MdixLocaleValidationIssue(baseKey, warning));
            }

            return issues;
        }

        // ── IDisposable ───────────────────────────────────────────────────────

        public void Dispose()
        {
            _db?.Dispose();
            _db = null;
        }

        // ── Private helpers ───────────────────────────────────────────────────

        private static MdixLocaleMetadata BuildMetadata(MdixDatabase db, string localeCode)
        {
            // GetEnumField reads the field name of an enum value ("ONE_OTHER", "LTR", etc.).
            // All keys are optional — sensible defaults applied when absent.
            return new MdixLocaleMetadata(
                bcp47:        db.GetString("locale_bcp47").UnwrapOr(localeCode),
                displayName:  db.GetString("locale_display_name").UnwrapOr(localeCode),
                pluralRule:   db.GetEnumField("locale_plural_rule").UnwrapOr("ONE_OTHER"),
                scriptDir:    db.GetEnumField("locale_script_dir").UnwrapOr("LTR"),
                genderSystem: db.GetEnumField("locale_gender_sys").UnwrapOr("NONE"),
                decimalSep:   db.GetString("fmt.decimal_sep").UnwrapOr("."),
                thousandsSep: db.GetString("fmt.thousands_sep").UnwrapOr(","),
                datePattern:  db.GetString("fmt.date_pattern").UnwrapOr("MM/DD/YYYY"));
        }

        // Indexed lookup for plain :: group arrays (used only when the named-form
        // path above finds nothing — all new locale files should use p2/p4 instead).
        //
        // v2 conventions:
        //   1-element:  always index 0
        //   2-element:  [0] = one form,  [1] = other form
        //   3+ element: [0] = zero form, [1] = one form, [2] = other/many form
        //
        // Note: v1 used [0]=other, [1]=one for 2-element arrays (reversed).
        // If you have existing 2-element locale arrays, swap the element order
        // or migrate to p2().
        private static int LegacyArrayIndex(int count, int arrayLength)
        {
            if (arrayLength == 1) return 0;

            if (arrayLength == 2)
                return count == 1 ? 0 : 1;

            // 3+ elements: [0]=zero, [1]=one, [2]=other/many
            if (count == 0) return 0;
            if (count == 1) return 1;
            return Math.Min(2, arrayLength - 1);
        }
    }
}
