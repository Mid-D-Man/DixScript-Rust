// csharp/src/MidManStudio.Mdix.Core/MdixMerge.cs
using System;
using System.Collections.Generic;
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
    /// All merge operations go through a JSON round-trip — DixScript-specific type
    /// metadata (HexColor markers, Blob wrappers, Regex wrappers) is preserved as
    /// string values in the merged output. Enum values are preserved as their resolved
    /// integers. QuickFunc formulas are not re-evaluated — the merged database contains
    /// only the resolved DATA values from each source.
    /// </summary>
    public static class MdixMerge
    {
        // ── Public API ────────────────────────────────────────────────────────

        /// <summary>
        /// Merges <paramref name="secondary"/> into <paramref name="primary"/> and
        /// returns a new database containing the combined result.
        /// Neither input database is modified or disposed.
        /// </summary>
        /// <param name="primary">The base database.</param>
        /// <param name="secondary">The database to merge in.</param>
        /// <param name="strategy">
        /// How to handle keys that exist in both databases.
        /// Defaults to <see cref="MdixMergeStrategy.PrimaryWins"/>.
        /// </param>
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

            return MdixConverter.FromJson(mergedJsonResult.SuccessResult);
        }

        /// <summary>
        /// Merges all databases in <paramref name="databases"/> left-to-right using
        /// <paramref name="strategy"/> and returns a single combined database.
        /// The first database in the sequence is the base. Each subsequent database
        /// is merged into the running result.
        /// Neither the inputs nor the intermediate results are disposed — only the
        /// final returned database requires disposal.
        /// </summary>
        /// <exception cref="ArgumentException">
        /// Thrown if <paramref name="databases"/> is empty.
        /// </exception>
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
                var singleJson = MdixConverter.ToJson(current, indented: false);
                if (singleJson.IsFailure) return MdixResult<MdixDatabase>.Err(singleJson.Error);
                return MdixConverter.FromJson(singleJson.SuccessResult);
            }

            return MdixResult<MdixDatabase>.Ok(current!);
        }

        /// <summary>
        /// Merges a raw JSON object string into an existing database and returns
        /// the combined database. Useful when the secondary source comes from an
        /// external JSON API rather than a loaded MdixDatabase.
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

            return MdixConverter.FromJson(mergedJsonResult.SuccessResult);
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
                    // Key only in secondary — always add it.
                    result[prop.Name] = prop.Value.Clone();
                    continue;
                }

                // Key exists in both.
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
                // PrimaryWins: existing value stays — no action needed.
            }

            return null;
        }

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
