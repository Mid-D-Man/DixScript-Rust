// com.midmanstudio.mdix.localization/Editor/Import/MdixLocaleImporter.cs
using System;
using System.Collections.Generic;
using System.IO;
using System.Text;
using MidManStudio.Mdix.Core;
using MidManStudio.Mdix.Unity;
using UnityEditor;

namespace MidManStudio.Mdix.Localization.Editor
{
    /// <summary>
    /// Converts CSV or JSON locale data into valid .mdix locale source strings.
    /// Use MdixLocalizationEditorWindow for the GUI front-end.
    ///
    /// Pipeline:
    ///   CSV / JSON → Dictionary of key → value
    ///             → GenerateMdixSource() → .mdix source string
    ///             → WriteAndImport() → MdixAsset
    ///
    /// Generated files include inline @ENUMS (no @IMPORTS dependency) so
    /// locale_plural_rule etc. are stored as enum values, keeping BuildMetadata's
    /// GetEnumField path working without a localization_helpers.mdix in scope.
    ///
    /// Excel: convert to CSV first (standard workflow), then use FromCsv.
    /// Multi-locale CSV: header row Key | en_US | fr_FR | ... produces one
    /// .mdix file per locale column via FromMultiLocaleCsv.
    /// </summary>
    public static class MdixLocaleImporter
    {
        // ── Known locale defaults ─────────────────────────────────────────────

        private static readonly Dictionary<string, LocaleMeta> _defaults =
            new Dictionary<string, LocaleMeta>(StringComparer.OrdinalIgnoreCase)
        {
            ["en_US"] = new LocaleMeta("English (US)",          "en-US", "ONE_OTHER",      "LTR", "NONE",     ".", ",",    "MM/DD/YYYY"),
            ["en_GB"] = new LocaleMeta("English (UK)",          "en-GB", "ONE_OTHER",      "LTR", "NONE",     ".", ",",    "DD/MM/YYYY"),
            ["en_AU"] = new LocaleMeta("English (Australia)",   "en-AU", "ONE_OTHER",      "LTR", "NONE",     ".", ",",    "DD/MM/YYYY"),
            ["fr_FR"] = new LocaleMeta("Français (France)",     "fr-FR", "ONE_OTHER",      "LTR", "NONE",     ",", "\u00a0","DD/MM/YYYY"),
            ["fr_CA"] = new LocaleMeta("Français (Canada)",     "fr-CA", "ONE_OTHER",      "LTR", "NONE",     ",", "\u00a0","YYYY-MM-DD"),
            ["de_DE"] = new LocaleMeta("Deutsch",               "de-DE", "ONE_OTHER",      "LTR", "NONE",     ",", ".",    "DD.MM.YYYY"),
            ["es_ES"] = new LocaleMeta("Español (España)",      "es-ES", "ONE_OTHER",      "LTR", "NONE",     ",", ".",    "DD/MM/YYYY"),
            ["es_MX"] = new LocaleMeta("Español (México)",      "es-MX", "ONE_OTHER",      "LTR", "NONE",     ".", ",",    "DD/MM/YYYY"),
            ["it_IT"] = new LocaleMeta("Italiano",              "it-IT", "ONE_OTHER",      "LTR", "NONE",     ",", ".",    "DD/MM/YYYY"),
            ["pt_BR"] = new LocaleMeta("Português (Brasil)",    "pt-BR", "ONE_OTHER",      "LTR", "NONE",     ",", ".",    "DD/MM/YYYY"),
            ["pt_PT"] = new LocaleMeta("Português",             "pt-PT", "ONE_OTHER",      "LTR", "NONE",     ",", ".",    "DD/MM/YYYY"),
            ["nl_NL"] = new LocaleMeta("Nederlands",            "nl-NL", "ONE_OTHER",      "LTR", "NONE",     ",", ".",    "DD-MM-YYYY"),
            ["pl_PL"] = new LocaleMeta("Polski",                "pl-PL", "SLAVIC",         "LTR", "FULL",     ",", "\u00a0","DD.MM.YYYY"),
            ["ru_RU"] = new LocaleMeta("Русский",               "ru-RU", "SLAVIC",         "LTR", "FULL",     ",", "\u00a0","DD.MM.YYYY"),
            ["uk_UA"] = new LocaleMeta("Українська",            "uk-UA", "SLAVIC",         "LTR", "FULL",     ",", "\u00a0","DD.MM.YYYY"),
            ["cs_CZ"] = new LocaleMeta("Čeština",               "cs-CZ", "SLAVIC",         "LTR", "FULL",     ",", "\u00a0","DD.MM.YYYY"),
            ["zh_CN"] = new LocaleMeta("中文 (简体)",             "zh-CN", "NONE",           "LTR", "NONE",     ".", ",",    "YYYY/MM/DD"),
            ["zh_TW"] = new LocaleMeta("中文 (繁體)",             "zh-TW", "NONE",           "LTR", "NONE",     ".", ",",    "YYYY/MM/DD"),
            ["ja_JP"] = new LocaleMeta("日本語",                 "ja-JP", "NONE",           "LTR", "NONE",     ".", ",",    "YYYY/MM/DD"),
            ["ko_KR"] = new LocaleMeta("한국어",                 "ko-KR", "NONE",           "LTR", "NONE",     ".", ",",    "YYYY.MM.DD"),
            ["ar_SA"] = new LocaleMeta("العربية",               "ar-SA", "ARABIC",         "RTL", "NONE",     ".", ",",    "DD/MM/YYYY"),
            ["he_IL"] = new LocaleMeta("עברית",                 "he-IL", "ONE_OTHER",      "RTL", "NONE",     ".", ",",    "DD/MM/YYYY"),
            ["tr_TR"] = new LocaleMeta("Türkçe",                "tr-TR", "ONE_OTHER",      "LTR", "NONE",     ",", ".",    "DD.MM.YYYY"),
        };

        // ── Public API ────────────────────────────────────────────────────────

        /// <summary>
        /// Import a single-locale translator CSV.
        /// Supported column layouts:
        ///   Key | Value                    (2-column)
        ///   Key | Note | Max | Value       (4-column, translator format)
        /// Note and Max are preserved in the export round-trip but otherwise ignored.
        /// Returns the generated .mdix source string.
        /// </summary>
        public static string FromCsv(string csv, string localeCode)
        {
            var rows    = ParseCsv(csv);
            var entries = new Dictionary<string, string>(StringComparer.Ordinal);

            if (rows.Count == 0) return GenerateMdixSource(entries, localeCode);

            bool hasHeader = rows[0].Length > 0 &&
                             rows[0][0].Equals("Key", StringComparison.OrdinalIgnoreCase);
            int startRow = hasHeader ? 1 : 0;
            int valueCol = hasHeader && rows[0].Length >= 4 ? 3 : 1;

            for (int r = startRow; r < rows.Count; r++)
            {
                var row = rows[r];
                if (row.Length == 0 || string.IsNullOrWhiteSpace(row[0])) continue;
                var key   = row[0].Trim();
                var value = row.Length > valueCol ? row[valueCol] : string.Empty;
                if (!string.IsNullOrEmpty(key))
                    entries[key] = value;
            }

            return GenerateMdixSource(entries, localeCode);
        }

        /// <summary>
        /// Import a multi-locale CSV where each column after Key is a locale code.
        /// Expected header: Key | en_US | fr_FR | ru_RU | ...
        /// Returns a dictionary of localeCode → generated .mdix source.
        /// </summary>
        public static Dictionary<string, string> FromMultiLocaleCsv(string csv)
        {
            var rows   = ParseCsv(csv);
            var result = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);

            if (rows.Count < 2) return result;

            var header      = rows[0];
            var localeCodes = new List<string>();
            for (int c = 1; c < header.Length; c++)
                localeCodes.Add(header[c].Trim());

            var columnData = new Dictionary<string, string>[localeCodes.Count];
            for (int i = 0; i < localeCodes.Count; i++)
                columnData[i] = new Dictionary<string, string>(StringComparer.Ordinal);

            for (int r = 1; r < rows.Count; r++)
            {
                var row = rows[r];
                if (row.Length == 0 || string.IsNullOrWhiteSpace(row[0])) continue;
                var key = row[0].Trim();
                for (int c = 0; c < localeCodes.Count; c++)
                {
                    var value = row.Length > c + 1 ? row[c + 1] : string.Empty;
                    if (!string.IsNullOrEmpty(key))
                        columnData[c][key] = value;
                }
            }

            for (int i = 0; i < localeCodes.Count; i++)
            {
                var code = localeCodes[i];
                if (!string.IsNullOrEmpty(code))
                    result[code] = GenerateMdixSource(columnData[i], code);
            }

            return result;
        }

        /// <summary>
        /// Import from a flat JSON object {"ui.play":"Jouer",...}.
        /// Uses MdixConverter.FromJson for validation then collects string leaves.
        /// Returns the generated .mdix source string.
        /// </summary>
        public static string FromJson(string json, string localeCode)
        {
            var dbResult = MdixConverter.FromJson(json);
            if (dbResult.IsFailure)
                throw new ArgumentException($"Invalid JSON: {dbResult.Error.Message}");

            using var db = dbResult.SuccessResult;

            var entries = new Dictionary<string, string>(StringComparer.Ordinal);
            CollectLeafStrings(db, string.Empty, entries, new HashSet<string>(StringComparer.Ordinal));
            return GenerateMdixSource(entries, localeCode);
        }

        /// <summary>
        /// Write a .mdix source string to disk and reimport it as a Unity MdixAsset.
        /// Creates intermediate directories as needed.
        /// Returns the imported MdixAsset, or null on failure.
        /// </summary>
        public static MdixAsset? WriteAndImport(string mdixSource, string assetPath)
        {
            if (!assetPath.EndsWith(".mdix", StringComparison.OrdinalIgnoreCase))
                assetPath += ".mdix";

            var dir = Path.GetDirectoryName(assetPath);
            if (!string.IsNullOrEmpty(dir) && !Directory.Exists(dir))
                Directory.CreateDirectory(dir);

            File.WriteAllText(assetPath, mdixSource, new UTF8Encoding(encoderShouldEmitUTF8Identifier: false));
            AssetDatabase.ImportAsset(assetPath);

            return AssetDatabase.LoadAssetAtPath<MdixAsset>(assetPath);
        }

        /// <summary>
        /// Returns locale metadata for a known locale code, or sensible ONE_OTHER defaults.
        /// </summary>
        public static LocaleMeta GetLocaleDefaults(string localeCode)
        {
            if (_defaults.TryGetValue(localeCode, out var meta))
                return meta;

            var bcp47 = localeCode.Length >= 2
                ? localeCode.Substring(0, 2).ToLowerInvariant() + "-" +
                  (localeCode.Length >= 5 ? localeCode.Substring(3, 2).ToUpperInvariant() : localeCode.ToUpperInvariant())
                : localeCode;

            return new LocaleMeta(localeCode, bcp47, "ONE_OTHER", "LTR", "NONE", ".", ",", "MM/DD/YYYY");
        }

        // ── .mdix source generation ───────────────────────────────────────────

        /// <summary>
        /// Generates a self-contained .mdix locale source from a flat key → value
        /// dictionary. Locale metadata keys (locale_*, fmt.*) are injected from
        /// the built-in defaults table when absent from the dictionary.
        /// Includes inline @ENUMS so locale_plural_rule etc. resolve as enum values
        /// via GetEnumField without requiring @IMPORTS(loc from "localization_helpers.mdix").
        /// </summary>
        public static string GenerateMdixSource(
            Dictionary<string, string> entries,
            string localeCode)
        {
            var meta = GetLocaleDefaults(localeCode);
            var sb   = new StringBuilder();

            // @CONFIG
            sb.AppendLine("@CONFIG(");
            sb.AppendLine($"  version -> \"2.0.0\"");
            sb.AppendLine($"  locale  -> \"{localeCode}\"");
            sb.AppendLine(")");
            sb.AppendLine();

            // @ENUMS — inline so enum-typed locale_* fields work without @IMPORTS.
            sb.AppendLine("@ENUMS(");
            sb.AppendLine("  PluralRule   { ONE_OTHER, ZERO_ONE_OTHER, SLAVIC, ARABIC, NONE }");
            sb.AppendLine("  ScriptDir    { LTR, RTL }");
            sb.AppendLine("  GenderSystem { NONE, MASC_FEM, FULL }");
            sb.AppendLine(")");
            sb.AppendLine();

            // @DATA — flat locale metadata first (two-tier rule), then user entries.
            sb.AppendLine("@DATA(");

            // Metadata flat props — use entry value when present, default otherwise.
            string M(string key, string def) => entries.TryGetValue(key, out var v) ? v : def;

            AppendFlat(sb,     "locale_display_name<string>", M("locale_display_name", meta.DisplayName));
            AppendFlat(sb,     "locale_bcp47<string>",        M("locale_bcp47",        meta.Bcp47));
            AppendEnumFlat(sb, "locale_plural_rule", "PluralRule",   M("locale_plural_rule", meta.PluralRule));
            AppendEnumFlat(sb, "locale_script_dir",  "ScriptDir",    M("locale_script_dir",  meta.ScriptDir));
            AppendEnumFlat(sb, "locale_gender_sys",  "GenderSystem", M("locale_gender_sys",  meta.GenderSystem));

            var decSep  = M("fmt.decimal_sep",   meta.DecimalSep);
            var thoSep  = M("fmt.thousands_sep", meta.ThousandsSep);
            var datPat  = M("fmt.date_pattern",  meta.DatePattern);
            sb.AppendLine($"  fmt: decimal_sep = \"{Esc(decSep)}\", " +
                          $"thousands_sep = \"{Esc(thoSep)}\", " +
                          $"date_pattern = \"{Esc(datPat)}\"");
            sb.AppendLine();

            // User-supplied entries: skip keys already written as metadata above.
            var metaKeys = new HashSet<string>(StringComparer.Ordinal)
            {
                "locale_display_name", "locale_bcp47",
                "locale_plural_rule",  "locale_script_dir", "locale_gender_sys",
                "fmt.decimal_sep",     "fmt.thousands_sep", "fmt.date_pattern",
            };

            // Group by first prefix segment. Top-level (no dot) → flat bucket.
            var flat   = new List<(string key, string value)>();
            var groups = new SortedDictionary<string, List<(string subkey, string value)>>(
                StringComparer.Ordinal);

            foreach (var kv in entries)
            {
                if (metaKeys.Contains(kv.Key)) continue;
                var dotIdx = kv.Key.IndexOf('.');
                if (dotIdx < 0)
                {
                    flat.Add((kv.Key, kv.Value));
                }
                else
                {
                    var prefix = kv.Key.Substring(0, dotIdx);
                    var subkey = kv.Key.Substring(dotIdx + 1);
                    if (!groups.TryGetValue(prefix, out var list))
                    {
                        list = new List<(string, string)>();
                        groups[prefix] = list;
                    }
                    list.Add((subkey, kv.Value));
                }
            }

            // Flat user entries first (two-tier: flat before grouped).
            foreach (var (k, v) in flat) AppendFlat(sb, k, v);
            if (flat.Count > 0 && groups.Count > 0) sb.AppendLine();

            // Grouped entries as table properties.
            foreach (var (prefix, props) in groups)
            {
                var parts = new StringBuilder();
                bool first = true;
                foreach (var (subkey, value) in props)
                {
                    if (!first) parts.Append(", ");
                    parts.Append($"{subkey} = \"{Esc(value)}\"");
                    first = false;
                }
                sb.AppendLine($"  {prefix}: {parts}");
            }

            sb.AppendLine(")");
            return sb.ToString();
        }

        // ── Private helpers ───────────────────────────────────────────────────

        private static void AppendFlat(StringBuilder sb, string key, string value) =>
            sb.AppendLine($"  {key} = \"{Esc(value)}\"");

        private static void AppendEnumFlat(StringBuilder sb, string key, string enumName, string field) =>
            sb.AppendLine($"  {key}<enum> = {enumName}.{field}");

        private static string Esc(string s) =>
            s.Replace("\\", "\\\\")
             .Replace("\"", "\\\"")
             .Replace("\n", "\\n")
             .Replace("\r", "\\r")
             .Replace("\t", "\\t");

        // Recursively collect leaf string values from a MdixDatabase.
        // Used by FromJson to flatten nested JSON before source generation.
        private static void CollectLeafStrings(
            MdixDatabase db, string prefix,
            Dictionary<string, string> entries,
            HashSet<string> visited)
        {
            var children = db.GetKeys(prefix).UnwrapOr(Array.Empty<string>());
            foreach (var child in children)
            {
                var fullKey = string.IsNullOrEmpty(prefix) ? child : $"{prefix}.{child}";
                if (visited.Contains(fullKey) || fullKey.Contains('[')) continue;

                var vt = db.GetValueType(fullKey);
                if (vt == MdixValueType.Object)
                {
                    CollectLeafStrings(db, fullKey, entries, visited);
                }
                else
                {
                    var str = db.GetString(fullKey);
                    if (str.IsSuccess) entries[fullKey] = str.SuccessResult;
                    visited.Add(fullKey);
                }
            }
        }

        // RFC 4180-compliant CSV parser. Handles quoted fields, doubled-quote
        // escaping, and CRLF/LF/CR line endings.
        private static List<string[]> ParseCsv(string csv)
        {
            var rows   = new List<string[]>();
            var fields = new List<string>();
            var field  = new StringBuilder();
            bool inQ   = false;
            int  i     = 0;

            void Flush()  { fields.Add(field.ToString()); field.Clear(); }
            void FlushRow()
            {
                if (fields.Count > 0)
                {
                    rows.Add(fields.ToArray());
                    fields.Clear();
                }
            }

            while (i < csv.Length)
            {
                var c = csv[i];
                if (inQ)
                {
                    if (c == '"')
                    {
                        if (i + 1 < csv.Length && csv[i + 1] == '"')
                        { field.Append('"'); i += 2; }
                        else { inQ = false; i++; }
                    }
                    else { field.Append(c); i++; }
                }
                else
                {
                    if      (c == '"')  { inQ = true; i++; }
                    else if (c == ',')  { Flush(); i++; }
                    else if (c == '\n') { Flush(); FlushRow(); i++; }
                    else if (c == '\r')
                    {
                        Flush(); FlushRow();
                        i += (i + 1 < csv.Length && csv[i + 1] == '\n') ? 2 : 1;
                    }
                    else { field.Append(c); i++; }
                }
            }

            Flush();
            if (fields.Count > 0) FlushRow();
            return rows;
        }

        // ── Nested data type ──────────────────────────────────────────────────

        public readonly struct LocaleMeta
        {
            public string DisplayName  { get; }
            public string Bcp47        { get; }
            public string PluralRule   { get; }
            public string ScriptDir    { get; }
            public string GenderSystem { get; }
            public string DecimalSep   { get; }
            public string ThousandsSep { get; }
            public string DatePattern  { get; }

            public LocaleMeta(
                string displayName, string bcp47, string pluralRule,
                string scriptDir,   string genderSystem,
                string decimalSep,  string thousandsSep, string datePattern)
            {
                DisplayName  = displayName;
                Bcp47        = bcp47;
                PluralRule   = pluralRule;
                ScriptDir    = scriptDir;
                GenderSystem = genderSystem;
                DecimalSep   = decimalSep;
                ThousandsSep = thousandsSep;
                DatePattern  = datePattern;
            }
        }
    }
}
