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
    /// // Build and save a structured config
    /// using var builder = Dix.Builder();
    /// builder
    ///     .Config(c => c.WithVersion("1.0.0"))
    ///     .Enums(e => e.WithEnum("AIType", "PASSIVE", "AGGRESSIVE", "BOSS"))
    ///     .Data(d => d
    ///         .WithString("app_name", "MyGame")
    ///         .WithTableProperties("server", t => t
    ///             .WithString("host", "localhost")
    ///             .WithInt("port", 8080))
    ///         .WithGroupArray("enemies", a => a
    ///             .AddObject(o => o.WithString("name", "Goblin").WithInt("hp", 50))
    ///             .AddObject(o => o.WithString("name", "Orc").WithInt("hp", 100))))
    ///     .Save("config.mdix")
    ///     .OrThrow();
    /// </code>
    /// </summary>
    public static class Dix
    {
        // ── Loading ───────────────────────────────────────────────────────────

        /// <summary>Loads a plain <c>.mdix</c> file from disk.</summary>
        public static Core.MdixResult<Core.MdixDatabase> Load(string path) =>
            Core.MdixDatabase.Load(path);

        /// <summary>Loads DixScript source from a raw string — no disk access.</summary>
        public static Core.MdixResult<Core.MdixDatabase> LoadStr(string source) =>
            Core.MdixDatabase.LoadStr(source);

        /// <summary>Loads an encrypted <c>.mdix.enc</c> file using a key file.</summary>
        public static Core.MdixResult<Core.MdixDatabase> LoadEncrypted(
            string  encPath,
            string? keyPath = null) =>
            Core.MdixDatabase.LoadEncrypted(encPath, keyPath);

        /// <summary>Loads an encrypted <c>.mdix.enc</c> file using a password.</summary>
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

        // ── Async Loading ─────────────────────────────────────────────────────

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
    }
}
