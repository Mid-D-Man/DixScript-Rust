using System;
using System.IO;
using UnityEngine;

namespace MidManStudio.Mdix.Unity
{
    /// <summary>
    /// Centralized path management for all mdix data on the current platform.
    ///
    /// All mdix data lives under a single root directory inside
    /// Application.persistentDataPath so it never collides with other plugins
    /// or engine files.
    ///
    /// Directory layout:
    ///   persistentDataPath/
    ///   └── mdix/
    ///       ├── saves/      player save data (.mdix)
    ///       ├── config/     mutable game config (.mdix)
    ///       ├── cache/      remote/compiled config cache (.mdix)
    ///       └── .keys/      locally stored key files (.mdix.key)
    ///
    /// Call MdixPaths.EnsureDirectoriesExist() once at game startup,
    /// or let each helper method create directories on demand.
    /// </summary>
    public static class MdixPaths
    {
        // ── Root ──────────────────────────────────────────────────────────────

        /// <summary>
        /// Root directory for all mdix runtime data.
        /// persistentDataPath/mdix/
        /// </summary>
        public static string Root =>
            Path.Combine(Application.persistentDataPath, "mdix");

        // ── Subdirectories ────────────────────────────────────────────────────

        /// <summary>
        /// Directory for player save data.
        /// persistentDataPath/mdix/saves/
        /// </summary>
        public static string Saves =>
            Path.Combine(Root, "saves");

        /// <summary>
        /// Directory for mutable game config files.
        /// Use this for configs that can change at runtime (difficulty settings,
        /// unlocked content flags, etc.).
        /// persistentDataPath/mdix/config/
        /// </summary>
        public static string Config =>
            Path.Combine(Root, "config");

        /// <summary>
        /// Directory for cached remote or compiled config files.
        /// Files here can be safely deleted and re-fetched.
        /// persistentDataPath/mdix/cache/
        /// </summary>
        public static string Cache =>
            Path.Combine(Root, "cache");

        /// <summary>
        /// Directory for locally stored key files.
        /// Dot-prefixed to signal these are not user-facing data files.
        /// On mobile this is inside the app sandbox. On PC it is not truly
        /// private — use cloud key retrieval for sensitive data on PC.
        /// persistentDataPath/mdix/.keys/
        /// </summary>
        public static string Keys =>
            Path.Combine(Root, ".keys");

        // ── File path builders ────────────────────────────────────────────────

        /// <summary>
        /// Full path for a save file.
        /// Appends .mdix extension if not already present.
        ///
        /// Example: MdixPaths.SaveFile("slot1") →
        ///   .../mdix/saves/slot1.mdix
        /// </summary>
        public static string SaveFile(string fileName) =>
            BuildFilePath(Saves, fileName, ".mdix");

        /// <summary>
        /// Full path for an encrypted save file.
        ///
        /// Example: MdixPaths.EncSaveFile("slot1") →
        ///   .../mdix/saves/slot1.mdix.enc
        /// </summary>
        public static string EncSaveFile(string fileName) =>
            BuildFilePath(Saves, fileName, ".mdix.enc");

        /// <summary>
        /// Full path for a config file.
        ///
        /// Example: MdixPaths.ConfigFile("difficulty") →
        ///   .../mdix/config/difficulty.mdix
        /// </summary>
        public static string ConfigFile(string fileName) =>
            BuildFilePath(Config, fileName, ".mdix");

        /// <summary>
        /// Full path for an encrypted config file.
        ///
        /// Example: MdixPaths.EncConfigFile("server_settings") →
        ///   .../mdix/config/server_settings.mdix.enc
        /// </summary>
        public static string EncConfigFile(string fileName) =>
            BuildFilePath(Config, fileName, ".mdix.enc");

        /// <summary>
        /// Full path for a cached file.
        ///
        /// Example: MdixPaths.CacheFile("remote_items") →
        ///   .../mdix/cache/remote_items.mdix
        /// </summary>
        public static string CacheFile(string fileName) =>
            BuildFilePath(Cache, fileName, ".mdix");

        /// <summary>
        /// Full path for a locally stored key file derived from
        /// the encrypted file name it belongs to.
        ///
        /// Example: MdixPaths.KeyFile("server_settings.mdix.enc") →
        ///   .../mdix/.keys/server_settings.mdix.key
        /// </summary>
        public static string KeyFile(string encFileName)
        {
            if (string.IsNullOrEmpty(encFileName))
                throw new ArgumentNullException(nameof(encFileName));

            // Strip .enc suffix if present, then append .key
            var baseName = Path.GetFileName(encFileName);
            if (baseName.EndsWith(".enc", StringComparison.OrdinalIgnoreCase))
                baseName = baseName.Substring(0, baseName.Length - 4);

            return Path.Combine(Keys, baseName + ".key");
        }

        /// <summary>
        /// Full path to a bundled read-only .mdix file in StreamingAssets.
        /// StreamingAssets is read-only at runtime on mobile — use this for
        /// bundled game data tables (enemies, items, abilities etc.).
        ///
        /// Example: MdixPaths.StreamingFile("enemies.mdix") →
        ///   .../StreamingAssets/mdix/enemies.mdix
        /// </summary>
        public static string StreamingFile(string relativePath)
        {
            if (string.IsNullOrEmpty(relativePath))
                throw new ArgumentNullException(nameof(relativePath));

            return Path.Combine(
                Application.streamingAssetsPath, "mdix", relativePath);
        }

        // ── Directory setup ───────────────────────────────────────────────────

        /// <summary>
        /// Creates all mdix subdirectories if they do not exist.
        /// Safe to call multiple times. Call once at game startup.
        ///
        /// Example:
        ///   void Awake() { MdixPaths.EnsureDirectoriesExist(); }
        /// </summary>
        public static void EnsureDirectoriesExist()
        {
            Directory.CreateDirectory(Saves);
            Directory.CreateDirectory(Config);
            Directory.CreateDirectory(Cache);
            Directory.CreateDirectory(Keys);
        }

        /// <summary>
        /// Returns true if a save file with the given name exists.
        /// </summary>
        public static bool SaveExists(string fileName) =>
            File.Exists(SaveFile(fileName));

        /// <summary>
        /// Returns true if a config file with the given name exists.
        /// </summary>
        public static bool ConfigExists(string fileName) =>
            File.Exists(ConfigFile(fileName));

        /// <summary>
        /// Returns true if a cached file with the given name exists.
        /// </summary>
        public static bool CacheExists(string fileName) =>
            File.Exists(CacheFile(fileName));

        /// <summary>
        /// Returns true if a locally stored key file for the given
        /// encrypted file name exists.
        /// </summary>
        public static bool KeyExists(string encFileName) =>
            File.Exists(KeyFile(encFileName));

        /// <summary>
        /// Delete all files in the cache directory.
        /// Safe to call — cache files can always be re-fetched.
        /// Does not delete saves, configs, or keys.
        /// </summary>
        public static void ClearCache()
        {
            if (!Directory.Exists(Cache)) return;

            foreach (var file in Directory.GetFiles(Cache))
            {
                try   { File.Delete(file); }
                catch { /* best effort */ }
            }
        }

        // ── Private helpers ───────────────────────────────────────────────────

        private static string BuildFilePath(
            string directory,
            string fileName,
            string extension)
        {
            if (string.IsNullOrEmpty(fileName))
                throw new ArgumentNullException(nameof(fileName));

            // Strip any existing mdix-related extension so callers can pass
            // either "slot1" or "slot1.mdix" and get the same result.
            var baseName = Path.GetFileNameWithoutExtension(
                fileName.EndsWith(".enc", StringComparison.OrdinalIgnoreCase)
                    ? Path.GetFileNameWithoutExtension(fileName)
                    : fileName);

            return Path.Combine(directory, baseName + extension);
        }
    }
}
