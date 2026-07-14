using System;
using System.Collections.Generic;
using System.Text;
using MidManStudio.Mdix.Core;

namespace MidManStudio.Mdix.Localization.Editor
{
    /// <summary>
    /// Exports a locale MdixDatabase to translator-friendly formats.
    ///
    /// ToCsv produces the 4-column translator format:
    ///   Key | Note | Max | Value
    /// Annotated keys (created with the key() @QUICKFUNCS helper) include their
    /// note and max_chars metadata. Plural forms appear as separate rows.
    /// The round-trip: export CSV → translate → FromCsv → import back.
    /// </summary>
    public static class MdixLocaleExporter
    {
        // Sub-key names produced by the key() quickfunc that should not
        // appear as standalone rows — they're metadata on the parent key.
        private static readonly HashSet<string> _skipSubKeys =
            new HashSet<string>(StringComparer.Ordinal)
            { "valid", "warning", "note", "max_chars" };

        // ── Public API ────────────────────────────────────────────────────────

        /// <summary>
        /// Export the locale database as a translator CSV.
        /// Columns: Key | Note | Max | Value
        /// </summary>
        public static string ToCsv(MdixDatabase db)
        {
            var sb      = new StringBuilder();
            var visited = new HashSet<string>(StringComparer.Ordinal);
            var rows    = new List<CsvRow>();

            sb.AppendLine("Key,Note,Max,Value");

            CollectRows(db, string.Empty, rows, visited, depth: 0);

            // Locale metadata rows first, then alphabetical.
            rows.Sort((a, b) =>
            {
                bool aM = a.Key.StartsWith("locale_", StringComparison.Ordinal) ||
                          a.Key.StartsWith("fmt.",     StringComparison.Ordinal);
                bool bM = b.Key.StartsWith("locale_", StringComparison.Ordinal) ||
                          b.Key.StartsWith("fmt.",     StringComparison.Ordinal);
                if (aM != bM) return aM ? -1 : 1;
                return StringComparer.Ordinal.Compare(a.Key, b.Key);
            });

            foreach (var row in rows)
            {
                sb.Append(Field(row.Key));   sb.Append(',');
                sb.Append(Field(row.Note));  sb.Append(',');
                sb.Append(Field(row.Max));   sb.Append(',');
                sb.AppendLine(Field(row.Value));
            }

            return sb.ToString();
        }

        /// <summary>
        /// Export the locale database as JSON via the existing MdixConverter.
        /// </summary>
        public static string ToJson(MdixDatabase db, bool indented = true) =>
            MdixConverter.ToJson(db, indented).UnwrapOr("{}");

        // ── Private helpers ───────────────────────────────────────────────────

        // Recursively walks the database prefix tree, collecting leaf string values
        // as CsvRow entries. Detects annotated keys (key().value siblings) and
        // emits their note and max_chars alongside the translated value.
        private static void CollectRows(
            MdixDatabase     db,
            string           prefix,
            List<CsvRow>     rows,
            HashSet<string>  visited,
            int              depth)
        {
            if (depth > 8) return; // safety guard

            var children = db.GetKeys(prefix).UnwrapOr(Array.Empty<string>());

            foreach (var child in children)
            {
                var fullKey = string.IsNullOrEmpty(prefix)
                    ? child
                    : $"{prefix}.{child}";

                if (visited.Contains(fullKey)) continue;

                // Indexed array elements look like key[0] — skip them here;
                // their named-form siblings (key.one, key.other) will be collected.
                if (fullKey.Contains('[')) { visited.Add(fullKey); continue; }

                // Skip sub-keys that are metadata from the key() quickfunc.
                if (_skipSubKeys.Contains(child)) { visited.Add(fullKey); continue; }

                var vt = db.GetValueType(fullKey);

                // Recurse into objects (table properties, plural entries, etc.).
                if (vt == MdixValueType.Object)
                {
                    CollectRows(db, fullKey, rows, visited, depth + 1);
                    visited.Add(fullKey);
                    continue;
                }

                // Skip the parent Array node itself; its named children are leaves.
                if (vt == MdixValueType.Array) { visited.Add(fullKey); continue; }

                // Detect annotated key pattern: child == "value" → this is key().value.
                // Read the parent's .note and .max_chars siblings.
                if (child.Equals("value", StringComparison.Ordinal) &&
                    !string.IsNullOrEmpty(prefix))
                {
                    var val  = db.GetString(fullKey).UnwrapOr(string.Empty);
                    var note = db.GetString($"{prefix}.note").UnwrapOr(string.Empty);
                    var maxR = db.GetInt($"{prefix}.max_chars");
                    var max  = maxR.IsSuccess ? maxR.SuccessResult.ToString() : string.Empty;

                    rows.Add(new CsvRow(prefix, note, max, val));

                    // Mark all sibling sub-keys as visited so they don't appear twice.
                    foreach (var s in new[] { "value", "note", "max_chars", "valid", "warning" })
                        visited.Add($"{prefix}.{s}");

                    continue;
                }

                // Plain leaf: string, enum field, date, hex, etc.
                var strResult  = db.GetString(fullKey);
                var enumResult = vt == MdixValueType.Enum
                    ? db.GetEnumField(fullKey)
                    : default;

                if (strResult.IsSuccess)
                    rows.Add(new CsvRow(fullKey, string.Empty, string.Empty, strResult.SuccessResult));
                else if (enumResult.IsSuccess)
                    rows.Add(new CsvRow(fullKey, string.Empty, string.Empty, enumResult.SuccessResult));

                visited.Add(fullKey);
            }
        }

        // RFC 4180 CSV field quoting.
        private static string Field(string value)
        {
            if (string.IsNullOrEmpty(value)) return string.Empty;
            bool needsQuotes = value.IndexOf(',')  >= 0 ||
                               value.IndexOf('"')  >= 0 ||
                               value.IndexOf('\n') >= 0 ||
                               value.IndexOf('\r') >= 0;
            return needsQuotes ? "\"" + value.Replace("\"", "\"\"") + "\"" : value;
        }

        private readonly struct CsvRow
        {
            public string Key   { get; }
            public string Note  { get; }
            public string Max   { get; }
            public string Value { get; }

            public CsvRow(string key, string note, string max, string value)
            {
                Key = key; Note = note; Max = max; Value = value;
            }
        }
    }
}
