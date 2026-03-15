// csharp/src/MidManStudio.Mdix.Core/MdixMerge.cs
using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Text.Json;

namespace MidManStudio.Mdix.Core
{
    /// <summary>
    /// Controls how key conflicts are resolved when merging two databases.
    /// </summary>
    public enum MdixMergeStrategy
    {
        /// <summary>Primary database keys win. Secondary fills gaps only.</summary>
        PrimaryWins,

        /// <summary>Secondary database keys overwrite primary on conflict.</summary>
        SecondaryWins,

        /// <summary>Any key present in both databases returns an error.</summary>
        ThrowOnConflict,
    }

    /// <summary>
    /// Merges two or more loaded DixScript databases into a single new database.
    ///
    /// Pipeline: both databases are exported via mdix_to_json which produces a
    /// flat hashmap serialised as JSON with dotted-path string keys
    /// (e.g. "server.host", "server.port").  The two JSON objects are deep-merged
    /// in managed C# code, then <see cref="LoadFromFlatDottedJson"/> rebuilds a
    /// valid @DATA(...) source string from those flat keys and loads it with
    /// MdixDatabase.LoadStr — bypassing the Rust mdix_from_json round-trip that
    /// cannot handle dotted-path keys as flat DixScript property names.
    /// </summary>
    public static class MdixMerge
    {
        // ── Public API ────────────────────────────────────────────────────────

        /// <summary>
        /// Merges <paramref name="secondary"/> into <paramref name="primary"/> and
        /// returns a new database containing the combined result.
        /// Neither input database is modified or disposed.
        /// </summary>
        public static MdixResult<MdixDatabase> Merge(
            MdixDatabase primary,
            MdixDatabase secondary,
            MdixMergeStrategy strategy = MdixMergeStrategy.PrimaryWins)
        {
            if (primary is null)   return MdixError.NativeError("Merge: primary cannot be null.");
            if (secondary is null) return MdixError.NativeError("Merge: secondary cannot be null.");

            var primaryJsonResult   = MdixConverter.ToJson(primary,   indented: false);
            var secondaryJsonResult = MdixConverter.ToJson(secondary, indented: false);

            if (primaryJsonResult.IsFailure)   return MdixResult<MdixDatabase>.Err(primaryJsonResult.Error);
            if (secondaryJsonResult.IsFailure) return MdixResult<MdixDatabase>.Err(secondaryJsonResult.Error);

            var mergedJsonResult = DeepMergeJson(
                primaryJsonResult.SuccessResult,
                secondaryJsonResult.SuccessResult,
                strategy);

            if (mergedJsonResult.IsFailure)
                return MdixResult<MdixDatabase>.Err(mergedJsonResult.Error);

            return LoadFromFlatDottedJson(mergedJsonResult.SuccessResult);
        }

        /// <summary>
        /// Merges all databases in <paramref name="databases"/> left-to-right using
        /// <paramref name="strategy"/> and returns a single combined database.
        /// </summary>
        public static MdixResult<MdixDatabase> MergeAll(
            IEnumerable<MdixDatabase> databases,
            MdixMergeStrategy strategy = MdixMergeStrategy.PrimaryWins)
        {
            if (databases is null)
                return MdixError.NativeError("MergeAll: databases cannot be null.");

            MdixDatabase? current = null;
            bool ownsCurrentDatabase = false;
            int index = 0;

            foreach (var db in databases)
            {
                if (db is null)
                {
                    if (ownsCurrentDatabase) current?.Dispose();
                    return MdixError.NativeError($"MergeAll: database at index {index} is null.");
                }

                if (index == 0)
                {
                    current = db;
                    ownsCurrentDatabase = false;
                    index++;
                    continue;
                }

                var mergeResult = Merge(current!, db, strategy);

                if (ownsCurrentDatabase) current?.Dispose();

                if (mergeResult.IsFailure)
                    return mergeResult;

                current = mergeResult.SuccessResult;
                ownsCurrentDatabase = true;
                index++;
            }

            if (index == 0)
                return MdixError.NativeError("MergeAll: databases sequence was empty.");

            if (index == 1 && current != null)
            {
                // Single database — return an independent copy via the same
                // flat-dotted-JSON path so nested structures are preserved.
                var singleJson = MdixConverter.ToJson(current, indented: false);
                if (singleJson.IsFailure) return MdixResult<MdixDatabase>.Err(singleJson.Error);
                return LoadFromFlatDottedJson(singleJson.SuccessResult);
            }

            return MdixResult<MdixDatabase>.Ok(current!);
        }

        /// <summary>
        /// Merges a raw JSON object string into an existing database and returns
        /// the combined database.
        /// </summary>
        public static MdixResult<MdixDatabase> MergeJson(
            MdixDatabase primary,
            string secondaryJson,
            MdixMergeStrategy strategy = MdixMergeStrategy.PrimaryWins)
        {
            if (primary is null)
                return MdixError.NativeError("MergeJson: primary cannot be null.");
            if (string.IsNullOrEmpty(secondaryJson))
                return MdixError.NativeError("MergeJson: secondaryJson cannot be null or empty.");

            var primaryJsonResult = MdixConverter.ToJson(primary, indented: false);
            if (primaryJsonResult.IsFailure)
                return MdixResult<MdixDatabase>.Err(primaryJsonResult.Error);

            var mergedJsonResult = DeepMergeJson(
                primaryJsonResult.SuccessResult,
                secondaryJson,
                strategy);

            if (mergedJsonResult.IsFailure)
                return MdixResult<MdixDatabase>.Err(mergedJsonResult.Error);

            return LoadFromFlatDottedJson(mergedJsonResult.SuccessResult);
        }

        // ── Private — JSON deep merge ─────────────────────────────────────────

        private static MdixResult<string> DeepMergeJson(
            string primaryJson,
            string secondaryJson,
            MdixMergeStrategy strategy)
        {
            try
            {
                using var primaryDoc   = JsonDocument.Parse(primaryJson);
                using var secondaryDoc = JsonDocument.Parse(secondaryJson);

                var primaryRoot   = primaryDoc.RootElement;
                var secondaryRoot = secondaryDoc.RootElement;

                if (primaryRoot.ValueKind != JsonValueKind.Object)
                    return MdixError.ParseError(
                        "Merge: primary database exported as non-object JSON — cannot merge.");

                if (secondaryRoot.ValueKind != JsonValueKind.Object)
                    return MdixError.ParseError(
                        "Merge: secondary database exported as non-object JSON — cannot merge.");

                var mergedDict = new Dictionary<string, JsonElement>();

                var mergeError = MergeObjects(
                    primaryRoot, secondaryRoot, mergedDict, strategy, rootPath: "");

                if (mergeError.HasValue)
                    return MdixResult<string>.Err(mergeError.Value);

                var serialized = SerializeMergedDict(mergedDict);
                return MdixResult<string>.Ok(serialized);
            }
            catch (JsonException ex)
            {
                return MdixError.ParseError($"Merge: JSON parsing failed — {ex.Message}");
            }
            catch (Exception ex)
            {
                return MdixError.NativeError($"Merge: unexpected error — {ex.Message}");
            }
        }

        private static MdixError? MergeObjects(
            JsonElement primary,
            JsonElement secondary,
            Dictionary<string, JsonElement> result,
            MdixMergeStrategy strategy,
            string rootPath)
        {
            // Add all primary keys first.
            foreach (var prop in primary.EnumerateObject())
                result[prop.Name] = prop.Value.Clone();

            // Process secondary keys.
            foreach (var prop in secondary.EnumerateObject())
            {
                var path = string.IsNullOrEmpty(rootPath)
                    ? prop.Name
                    : $"{rootPath}.{prop.Name}";

                if (!result.TryGetValue(prop.Name, out var existing))
                {
                    result[prop.Name] = prop.Value.Clone();
                    continue;
                }

                if (strategy == MdixMergeStrategy.ThrowOnConflict)
                {
                    return MdixError.NativeError(
                        $"Merge conflict at '{path}': key exists in both databases " +
                        $"and strategy is ThrowOnConflict.");
                }

                // Both are objects — recurse.
                if (existing.ValueKind == JsonValueKind.Object
                    && prop.Value.ValueKind == JsonValueKind.Object)
                {
                    var nestedPrimary   = strategy == MdixMergeStrategy.PrimaryWins
                        ? existing        : prop.Value;
                    var nestedSecondary = strategy == MdixMergeStrategy.PrimaryWins
                        ? prop.Value      : existing;

                    var nested = new Dictionary<string, JsonElement>();
                    var err    = MergeObjects(nestedPrimary, nestedSecondary, nested, strategy, path);
                    if (err.HasValue) return err;

                    result[prop.Name] = JsonSerializer.SerializeToElement(
                        ConvertDictToObject(nested));
                    continue;
                }

                // Scalar or array conflict — apply strategy.
                if (strategy == MdixMergeStrategy.SecondaryWins)
                    result[prop.Name] = prop.Value.Clone();
                // PrimaryWins: existing value stays.
            }

            return null;
        }

        // ── Private — database reconstruction from flat-dotted JSON ───────────

        /// <summary>
        /// Rebuilds a <see cref="MdixDatabase"/> from the flat-dotted-key JSON that
        /// <c>mdix_to_json</c> produces (e.g. <c>{"server.host":"x","server.port":8080}</c>).
        ///
        /// Groups dotted keys back into DixScript table-property syntax
        /// (<c>server: host = "x", port = 8080</c>) so the DixLoader stores them
        /// as the expected flat dotted paths internally.  Array values are emitted
        /// with the group-array <c>::</c> syntax.  Indexed array item keys
        /// (e.g. <c>tags[0]</c>) are skipped — they are redundant with the array.
        ///
        /// Two-tier ordering is respected: flat scalar properties first, then
        /// table groups, then group arrays.
        /// </summary>
        private static MdixResult<MdixDatabase> LoadFromFlatDottedJson(string json)
        {
            Dictionary<string, JsonElement> entries;
            try
            {
                using var doc = JsonDocument.Parse(json);
                if (doc.RootElement.ValueKind != JsonValueKind.Object)
                    return MdixError.ParseError(
                        "LoadFromFlatDottedJson: merged JSON root must be an object.");

                entries = new Dictionary<string, JsonElement>(StringComparer.Ordinal);
                foreach (var prop in doc.RootElement.EnumerateObject())
                    entries[prop.Name] = prop.Value.Clone();
            }
            catch (JsonException ex)
            {
                return MdixError.ParseError(
                    $"LoadFromFlatDottedJson: JSON parse error: {ex.Message}");
            }

            var flatScalars = new List<(string key, JsonElement value)>();
            var tableGroups = new Dictionary<string, List<(string subKey, JsonElement value)>>(
                StringComparer.Ordinal);
            var groupArrays = new List<(string key, JsonElement value)>();

            foreach (var kvp in entries)
            {
                var key   = kvp.Key;
                var value = kvp.Value;

                // Skip indexed array item keys like "tags[0]" — the array entry
                // itself ("tags") carries the full set of items.
                if (key.Contains('['))
                    continue;

                switch (value.ValueKind)
                {
                    case JsonValueKind.Array:
                        groupArrays.Add((key, value));
                        break;

                    case JsonValueKind.Object:
                        // Inline nested object (e.g. from real nested JSON passed to
                        // MergeJson) — expand one level into a table group.
                        if (!tableGroups.TryGetValue(key, out var objGroup))
                        {
                            objGroup = new List<(string, JsonElement)>();
                            tableGroups[key] = objGroup;
                        }
                        foreach (var prop in value.EnumerateObject())
                            objGroup.Add((prop.Name, prop.Value));
                        break;

                    default:
                    {
                        int lastDot = key.LastIndexOf('.');
                        if (lastDot < 0)
                        {
                            // Simple top-level key — flat property.
                            flatScalars.Add((key, value));
                        }
                        else
                        {
                            // Dotted key — group by prefix into a table property.
                            var prefix = key.Substring(0, lastDot);
                            var subKey = key.Substring(lastDot + 1);
                            if (!tableGroups.TryGetValue(prefix, out var tGroup))
                            {
                                tGroup = new List<(string, JsonElement)>();
                                tableGroups[prefix] = tGroup;
                            }
                            tGroup.Add((subKey, value));
                        }
                        break;
                    }
                }
            }

            // Build @DATA(...) with two-tier ordering:
            //   tier 1 — flat scalar properties
            //   tier 2 — table group properties + group arrays
            var sb = new StringBuilder();
            sb.AppendLine("@DATA(");

            foreach (var (key, value) in flatScalars)
            {
                sb.Append("  ");
                sb.Append(key);
                sb.Append(" = ");
                sb.AppendLine(FormatMdixScalar(value));
            }

            foreach (var (prefix, props) in tableGroups)
            {
                if (props.Count == 0) continue;
                sb.Append("  ");
                sb.Append(prefix);
                sb.Append(": ");
                sb.AppendLine(string.Join(", ",
                    props.Select(p => $"{p.subKey} = {FormatMdixScalar(p.value)}")));
            }

            foreach (var (key, arr) in groupArrays)
            {
                sb.Append("  ");
                sb.Append(key);
                sb.Append(":: ");
                var items = new List<string>();
                foreach (var el in arr.EnumerateArray())
                    items.Add(FormatMdixArrayItem(el));
                sb.AppendLine(string.Join(", ", items));
            }

            sb.Append(")");

            return MdixDatabase.LoadStr(sb.ToString());
        }

        /// <summary>
        /// Formats a scalar <see cref="JsonElement"/> as a DixScript value literal.
        /// </summary>
        private static string FormatMdixScalar(JsonElement el)
        {
            return el.ValueKind switch
            {
                JsonValueKind.True   => "true",
                JsonValueKind.False  => "false",
                JsonValueKind.Null   => "null",
                JsonValueKind.String =>
                    $"\"{EscapeMdixString(el.GetString() ?? string.Empty)}\"",
                JsonValueKind.Number =>
                    el.TryGetInt64(out var lv)
                        ? lv.ToString(System.Globalization.CultureInfo.InvariantCulture)
                        : el.GetDouble()
                             .ToString(System.Globalization.CultureInfo.InvariantCulture),
                _ => "null",
            };
        }

        /// <summary>
        /// Formats an array item <see cref="JsonElement"/> as a DixScript value literal.
        /// Objects are emitted as inline object literals <c>{ key = val, ... }</c>.
        /// </summary>
        private static string FormatMdixArrayItem(JsonElement el)
        {
            if (el.ValueKind == JsonValueKind.Object)
            {
                var pairs = new List<string>();
                foreach (var prop in el.EnumerateObject())
                    pairs.Add($"{prop.Name} = {FormatMdixScalar(prop.Value)}");
                return $"{{ {string.Join(", ", pairs)} }}";
            }
            return FormatMdixScalar(el);
        }

        /// <summary>
        /// Escapes a string value for use inside DixScript double-quoted literals.
        /// </summary>
        private static string EscapeMdixString(string s) =>
            s.Replace("\\", "\\\\")
             .Replace("\"", "\\\"")
             .Replace("\n", "\\n")
             .Replace("\r", "\\r")
             .Replace("\t", "\\t");

        // ── Private helpers — JSON utility ────────────────────────────────────

        private static Dictionary<string, object?> ConvertDictToObject(
            Dictionary<string, JsonElement> dict)
        {
            var obj = new Dictionary<string, object?>(dict.Count);
            foreach (var kv in dict)
                obj[kv.Key] = ConvertElement(kv.Value);
            return obj;
        }

        private static object? ConvertElement(JsonElement el)
        {
            return el.ValueKind switch
            {
                JsonValueKind.Null    => null,
                JsonValueKind.True    => true,
                JsonValueKind.False   => false,
                JsonValueKind.Number  => el.TryGetInt64(out var l) ? (object)l : el.GetDouble(),
                JsonValueKind.String  => el.GetString(),
                JsonValueKind.Array   => ConvertArray(el),
                JsonValueKind.Object  => ConvertObject(el),
                _                    => el.GetRawText(),
            };
        }

        private static List<object?> ConvertArray(JsonElement el)
        {
            var list = new List<object?>(el.GetArrayLength());
            foreach (var item in el.EnumerateArray())
                list.Add(ConvertElement(item));
            return list;
        }

        private static Dictionary<string, object?> ConvertObject(JsonElement el)
        {
            var dict = new Dictionary<string, object?>();
            foreach (var prop in el.EnumerateObject())
                dict[prop.Name] = ConvertElement(prop.Value);
            return dict;
        }

        private static string SerializeMergedDict(Dictionary<string, JsonElement> dict)
        {
            var options = new JsonSerializerOptions { WriteIndented = false };
            var obj     = ConvertDictToObject(dict);
            return JsonSerializer.Serialize(obj, options);
        }
    }
}
