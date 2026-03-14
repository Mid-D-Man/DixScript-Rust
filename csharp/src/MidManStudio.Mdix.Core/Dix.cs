using System.Threading;
using System.Threading.Tasks;

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

        /// <summary>
        /// Parses a JSON object string and returns a loaded database.
        /// The JSON must be an object at the top level. Dispose when done.
        /// </summary>
        public static Core.MdixResult<Core.MdixDatabase> LoadJson(string json) =>
            Core.MdixConverter.FromJson(json);

        /// <summary>
        /// Parses a TOML table string and returns a loaded database.
        /// The TOML must be a table at the top level. Dispose when done.
        /// </summary>
        public static Core.MdixResult<Core.MdixDatabase> LoadToml(string toml) =>
            Core.MdixConverter.FromToml(toml);

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

        /// <summary>Re-serialize a loaded database to .mdix text.</summary>
        public static Core.MdixResult<string> ToMdix(
            Core.MdixDatabase db,
            Core.MdixFormatMode mode = Core.MdixFormatMode.Default) =>
            Core.MdixConverter.ToMdix(db, mode);

        /// <summary>Export a loaded database as JSON.</summary>
        public static Core.MdixResult<string> ToJson(Core.MdixDatabase db, bool indented = true) =>
            Core.MdixConverter.ToJson(db, indented);

        /// <summary>Export a loaded database as TOML.</summary>
        public static Core.MdixResult<string> ToToml(Core.MdixDatabase db) =>
            Core.MdixConverter.ToToml(db);

        /// <summary>Format a raw .mdix source string.</summary>
        public static Core.MdixResult<string> Format(
            string source,
            Core.MdixFormatMode mode = Core.MdixFormatMode.Default) =>
            Core.MdixConverter.FormatSource(source, mode);

        /// <summary>Minify a raw .mdix source string.</summary>
        public static Core.MdixResult<string> Minify(string source) =>
            Core.MdixConverter.MinifySource(source);

        // ── Serializer cache ──────────────────────────────────────────────────

        public static void ClearSerializerCache() => Core.MdixSerializer.ClearCache();
    }
}
