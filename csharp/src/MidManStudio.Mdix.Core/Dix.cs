// csharp/src/MidManStudio.Mdix.Core/Dix.cs
using System.Threading;
using System.Threading.Tasks;
using System.Collections.Generic;

namespace MidManStudio.Mdix
{
    /// <summary>
    /// Static one-liner facade — the primary entry point for all callers.
    /// </summary>
    public static class Dix
    {
        // ── Loading ───────────────────────────────────────────────────────────

        public static Core.MdixResult<Core.MdixDatabase> Load(string path) =>
            Core.MdixDatabase.Load(path);

        public static Core.MdixResult<Core.MdixDatabase> LoadStr(string source) =>
            Core.MdixDatabase.LoadStr(source);

        public static Core.MdixResult<Core.MdixDatabase> LoadEncrypted(
            string encPath, string? keyPath = null) =>
            Core.MdixDatabase.LoadEncrypted(encPath, keyPath);

        public static Core.MdixResult<Core.MdixDatabase> LoadEncryptedPassword(
            string encPath, string password) =>
            Core.MdixDatabase.LoadEncryptedPassword(encPath, password);

        public static Core.MdixResult<Core.MdixDatabase> LoadEncryptedBytes(
            byte[] data, string keyContent, string? password = null) =>
            Core.MdixDatabase.LoadEncryptedBytes(data, keyContent, password);

        public static Core.MdixResult<Core.MdixDatabase> LoadEncryptedWith(
            string encPath, Core.MdixLoadOptions options) =>
            options.Apply(encPath);

        // ── Async loading ─────────────────────────────────────────────────────

        public static Task<Core.MdixResult<Core.MdixDatabase>> LoadAsync(
            string path, CancellationToken ct = default) =>
            Core.MdixDatabase.LoadAsync(path, ct);

        public static Task<Core.MdixResult<Core.MdixDatabase>> LoadStrAsync(
            string source, CancellationToken ct = default) =>
            Core.MdixDatabase.LoadStrAsync(source, ct);

        public static Task<Core.MdixResult<Core.MdixDatabase>> LoadEncryptedAsync(
            string encPath, string? keyPath = null, CancellationToken ct = default) =>
            Core.MdixDatabase.LoadEncryptedAsync(encPath, keyPath, ct);

        public static Task<Core.MdixResult<Core.MdixDatabase>> LoadEncryptedPasswordAsync(
            string encPath, string password, CancellationToken ct = default) =>
            Core.MdixDatabase.LoadEncryptedPasswordAsync(encPath, password, ct);

        public static Task<Core.MdixResult<Core.MdixDatabase>> LoadEncryptedBytesAsync(
            byte[] data, string keyContent, string? password = null, CancellationToken ct = default) =>
            Core.MdixDatabase.LoadEncryptedBytesAsync(data, keyContent, password, ct);

        // ── Foreign format loading ────────────────────────────────────────────

        public static Core.MdixResult<Core.MdixDatabase> LoadJson(string json) =>
            Core.MdixConverter.FromJson(json);

        public static Core.MdixResult<Core.MdixDatabase> LoadToml(string toml) =>
            Core.MdixConverter.FromToml(toml);

        // ── Merging ───────────────────────────────────────────────────────────

        /// <summary>
        /// Merges <paramref name="secondary"/> into <paramref name="primary"/> and
        /// returns a new combined database. Neither input is modified or disposed.
        /// </summary>
        public static Core.MdixResult<Core.MdixDatabase> Merge(
            Core.MdixDatabase primary,
            Core.MdixDatabase secondary,
            Core.MdixMergeStrategy strategy = Core.MdixMergeStrategy.PrimaryWins) =>
            Core.MdixMerge.Merge(primary, secondary, strategy);

        /// <summary>
        /// Merges all databases left-to-right. First is the base; each subsequent
        /// database is merged in using <paramref name="strategy"/>.
        /// </summary>
        public static Core.MdixResult<Core.MdixDatabase> MergeAll(
            IEnumerable<Core.MdixDatabase> databases,
            Core.MdixMergeStrategy strategy = Core.MdixMergeStrategy.PrimaryWins) =>
            Core.MdixMerge.MergeAll(databases, strategy);

        /// <summary>
        /// Merges a raw JSON object string into an existing database.
        /// </summary>
        public static Core.MdixResult<Core.MdixDatabase> MergeJson(
            Core.MdixDatabase primary,
            string secondaryJson,
            Core.MdixMergeStrategy strategy = Core.MdixMergeStrategy.PrimaryWins) =>
            Core.MdixMerge.MergeJson(primary, secondaryJson, strategy);

        // ── Async merging ─────────────────────────────────────────────────────

        public static Task<Core.MdixResult<Core.MdixDatabase>> MergeAsync(
            Core.MdixDatabase primary,
            Core.MdixDatabase secondary,
            Core.MdixMergeStrategy strategy = Core.MdixMergeStrategy.PrimaryWins,
            CancellationToken ct = default) =>
            Task.Run(() => Core.MdixMerge.Merge(primary, secondary, strategy), ct);

        public static Task<Core.MdixResult<Core.MdixDatabase>> MergeAllAsync(
            IEnumerable<Core.MdixDatabase> databases,
            Core.MdixMergeStrategy strategy = Core.MdixMergeStrategy.PrimaryWins,
            CancellationToken ct = default) =>
            Task.Run(() => Core.MdixMerge.MergeAll(databases, strategy), ct);

        // ── POCO deserialization ──────────────────────────────────────────────

        public static Core.MdixResult<T> Deserialize<T>(string path, string? prefix = null)
        {
            var loadResult = Load(path);
            if (loadResult.IsFailure) return Core.MdixResult<T>.Err(loadResult.Error);
            using var db = loadResult.SuccessResult;
            return db.Deserialize<T>(prefix);
        }

        public static Core.MdixResult<T> DeserializeFrom<T>(
            Core.MdixDatabase db, string? prefix = null) =>
            db.Deserialize<T>(prefix);

        // ── Building ──────────────────────────────────────────────────────────

        public static Core.MdixBuilder Builder() => Core.MdixBuilder.Create();

        public static Core.MdixResult<Core.MdixBuilder> BuilderFrom(Core.MdixDatabase db) =>
            Core.MdixBuilder.FromDatabase(db);

        // ── Conversion and formatting ─────────────────────────────────────────

        public static Core.MdixResult<string> ToMdix(
            Core.MdixDatabase db,
            Core.MdixFormatMode mode = Core.MdixFormatMode.Default) =>
            Core.MdixConverter.ToMdix(db, mode);

        public static Core.MdixResult<string> ToJson(Core.MdixDatabase db, bool indented = true) =>
            Core.MdixConverter.ToJson(db, indented);

        public static Core.MdixResult<string> ToToml(Core.MdixDatabase db) =>
            Core.MdixConverter.ToToml(db);

        public static Core.MdixResult<string> Format(
            string source,
            Core.MdixFormatMode mode = Core.MdixFormatMode.Default) =>
            Core.MdixConverter.FormatSource(source, mode);

        public static Core.MdixResult<string> Minify(string source) =>
            Core.MdixConverter.MinifySource(source);

        // ── Serializer cache ──────────────────────────────────────────────────

        public static void ClearSerializerCache() => Core.MdixSerializer.ClearCache();
    }
}
