using System.Threading;
using System.Threading.Tasks;

namespace MidManStudio.Mdix
{
    /// <summary>
    /// Static one-liner facade — the primary entry point for all callers.
    /// <code>
    /// // Load and read
    /// using var db = Dix.Load("config.mdix").OrThrow();
    /// int port = db.GetInt("server.port").UnwrapOr(8080);
    ///
    /// // Deserialize to a POCO
    /// var cfg = Dix.Deserialize&lt;ServerConfig&gt;("config.mdix").OrThrow();
    ///
    /// // Build and save
    /// using var builder = Dix.Builder();
    /// builder.Data(d => d.WithString("app", "MyGame")).Save("out.mdix").OrThrow();
    ///
    /// // Serialize a POCO into a builder
    /// using var builder = Dix.Builder();
    /// builder.Serialize(myConfig).OrThrow();
    /// builder.Save("config.mdix").OrThrow();
    /// </code>
    /// </summary>
    public static class Dix
    {
        // ── Loading ───────────────────────────────────────────────────────────

        /// <summary>Loads a plain .mdix file from disk.</summary>
        public static Core.MdixResult<Core.MdixDatabase> Load(string path) =>
            Core.MdixDatabase.Load(path);

        /// <summary>Loads DixScript source from a raw string — no disk access.</summary>
        public static Core.MdixResult<Core.MdixDatabase> LoadStr(string source) =>
            Core.MdixDatabase.LoadStr(source);

        /// <summary>Loads an encrypted .mdix.enc file using a key file.</summary>
        public static Core.MdixResult<Core.MdixDatabase> LoadEncrypted(
            string  encPath,
            string? keyPath = null) =>
            Core.MdixDatabase.LoadEncrypted(encPath, keyPath);

        /// <summary>Loads an encrypted .mdix.enc file using a password.</summary>
        public static Core.MdixResult<Core.MdixDatabase> LoadEncryptedPassword(
            string encPath,
            string password) =>
            Core.MdixDatabase.LoadEncryptedPassword(encPath, password);

        /// <summary>Loads encrypted data from raw bytes.</summary>
        public static Core.MdixResult<Core.MdixDatabase> LoadEncryptedBytes(
            byte[]  data,
            string  keyContent,
            string? password = null) =>
            Core.MdixDatabase.LoadEncryptedBytes(data, keyContent, password);

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

        // ── POCO deserialization ───────────────────────────────────────────────

        /// <summary>
        /// Loads a .mdix file and deserializes it directly into a strongly-typed object.
        /// Equivalent to calling <see cref="Load"/> then <see cref="Core.MdixDatabase.Deserialize{T}"/>.
        /// The database is created and disposed internally — do not use it after this call.
        /// </summary>
        /// <param name="path">Path to the .mdix file.</param>
        /// <param name="prefix">
        /// Optional root path prefix. Overrides any <see cref="Core.MdixObjectAttribute"/> on T.
        /// </param>
        public static Core.MdixResult<T> Deserialize<T>(string path, string? prefix = null)
        {
            var loadResult = Load(path);
            if (loadResult.IsFailure)
                return Core.MdixResult<T>.Err(loadResult.Error);

            using var db = loadResult.SuccessResult;
            return db.Deserialize<T>(prefix);
        }

        /// <summary>
        /// Deserializes an already-loaded database into a strongly-typed object.
        /// The database remains open and valid after this call.
        /// </summary>
        /// <param name="db">The loaded database to read from.</param>
        /// <param name="prefix">
        /// Optional root path prefix. Overrides any <see cref="Core.MdixObjectAttribute"/> on T.
        /// </param>
        public static Core.MdixResult<T> DeserializeFrom<T>(
            Core.MdixDatabase db,
            string?           prefix = null) =>
            db.Deserialize<T>(prefix);

        // ── Building ──────────────────────────────────────────────────────────

        /// <summary>
        /// Creates a new empty <see cref="Core.MdixBuilder"/> for constructing
        /// .mdix config files with @CONFIG, @ENUMS, and @DATA sections.
        /// </summary>
        public static Core.MdixBuilder Builder() =>
            Core.MdixBuilder.Create();

        /// <summary>
        /// Creates a <see cref="Core.MdixBuilder"/> pre-populated with DATA entries
        /// copied from a loaded database.
        /// </summary>
        public static Core.MdixResult<Core.MdixBuilder> BuilderFrom(Core.MdixDatabase db) =>
            Core.MdixBuilder.FromDatabase(db);

        // ── Serializer cache ──────────────────────────────────────────────────

        /// <summary>
        /// Clears the internal reflection cache used by <see cref="Deserialize{T}"/>
        /// and <see cref="Core.MdixBuilder.Serialize{T}"/>.
        /// Call this after hot-reload or dynamic assembly loading to force the
        /// serializer to re-inspect types on the next use.
        /// </summary>
        public static void ClearSerializerCache() => Core.MdixSerializer.ClearCache();
    }
}
