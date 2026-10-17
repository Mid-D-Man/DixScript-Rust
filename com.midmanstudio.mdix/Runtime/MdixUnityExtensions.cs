using System;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using System.Collections;
using System.Collections.Generic;
using System.Text;
using UnityEngine;
using MidManStudio.Mdix.Core;

namespace MidManStudio.Mdix.Unity
{
    /// <summary>
    /// Unity-specific helpers layered on top of the Core Dix API.
    /// Handles coroutine loading, platform-correct save paths via MdixPaths,
    /// and main-thread callback dispatch for async operations.
    /// </summary>
    public static class MdixUnityExtensions
    {
        // ── MdixAsset load helpers ────────────────────────────────────────────

        /// <summary>
        /// Load a MdixDatabase from a MdixAsset reference.
        /// The caller must dispose the returned database.
        /// </summary>
        public static MdixResult<MdixDatabase> LoadFrom(this MdixAsset asset)
        {
            if (asset == null)
                return MdixError.NativeError(
                    "LoadFrom: asset reference is null. " +
                    "Assign a .mdix asset in the Inspector.");

            return asset.Load();
        }

        /// <summary>
        /// Read a MdixAsset directly into a class or struct of type T (fields, properties, lists,
        /// dictionaries, Unity math types). No database handle to manage.
        /// </summary>
        public static MdixResult<T> LoadAs<T>(
            this MdixAsset asset, string? prefix = null)
        {
            if (asset == null)
                return MdixError.NativeError("LoadAs: asset reference is null.");

            return asset.LoadAs<T>(prefix);
        }

        // ── Coroutine loading ─────────────────────────────────────────────────

        /// <summary>
        /// Load a .mdix file via coroutine.
        /// On Android StreamingAssets requires UnityWebRequest — this handles
        /// that automatically based on platform and path.
        ///
        /// Usage:
        ///   yield return MdixUnityExtensions.LoadCoroutine(
        ///       MdixPaths.StreamingFile("enemies.mdix"),
        ///       result => { using var db = result.OrThrow(); ... });
        /// </summary>
        public static IEnumerator LoadCoroutine(
            string                           path,
            Action<MdixResult<MdixDatabase>> onComplete)
        {
            if (onComplete == null)
                throw new ArgumentNullException(nameof(onComplete));

            MdixResult<MdixDatabase>? result = null;

#if UNITY_ANDROID && !UNITY_EDITOR
            var www = UnityEngine.Networking.UnityWebRequest.Get(path);
            yield return www.SendWebRequest();

            result = www.result != UnityEngine.Networking.UnityWebRequest.Result.Success
                ? MdixError.IoError($"LoadCoroutine: {www.error}")
                : Dix.LoadStr(www.downloadHandler.text);

            www.Dispose();
#else
            var task = Task.Run(() => Dix.Load(path));
            while (!task.IsCompleted) yield return null;
            result = task.Result;
#endif
            onComplete(result);
        }

        // ── Async with main-thread callback ───────────────────────────────────

        /// <summary>
        /// Asynchronously load a .mdix file, invoking the callback on the
        /// Unity main thread when complete.
        /// </summary>
        public static async Task LoadAsync(
            string                           path,
            Action<MdixResult<MdixDatabase>> onComplete,
            CancellationToken                ct = default)
        {
            if (onComplete == null)
                throw new ArgumentNullException(nameof(onComplete));

            var result = await Dix.LoadAsync(path, ct).ConfigureAwait(false);
            await MainThreadDispatcher.RunOnMainThreadAsync(
                () => onComplete(result));
        }

        // ── Save helpers ──────────────────────────────────────────────────────

        /// <summary>
        /// Save a class or struct to the mdix saves directory. Directories are created automatically.
        ///
        /// Stored: public fields, [SerializeField] fields and properties; every number type, strings,
        /// enums, Guid, DateTime, TimeSpan; arrays, lists, sets and dictionaries; nested classes, structs
        /// and records; Vector2/3/4, Quaternion, Matrix4x4, Rect, Bounds and Color. See <see cref="MdixWriter"/>.
        /// What cannot be stored (Unity object references, delegates) is left out and logged once.
        ///
        /// The file is written to a temporary name first and swapped in, and the previous save is kept
        /// as "name.mdix.bak", so a crash or a full disk cannot leave a half-written save.
        ///
        /// Example:
        ///   MdixUnityExtensions.Save("slot1", playerData);
        ///   // writes to: persistentDataPath/mdix/saves/slot1.mdix
        /// </summary>
        public static MdixResult<Unit> Save<T>(
            string  fileName,
            T       data,
            string? prefix = null)
        {
            if (data == null)
                return MdixError.NativeError("Save: data cannot be null.");

            MdixPaths.EnsureDirectoriesExist();
            return WriteFile("Save", MdixPaths.SaveFile(fileName), data, prefix);
        }

        /// <summary>
        /// Load a class or struct from a save file in the mdix saves directory.
        /// Returns a failed result if the file does not exist —
        /// this is normal on first launch, not an error.
        /// If the file is damaged, the backup from the previous save is tried.
        ///
        /// Example:
        ///   var player = MdixUnityExtensions
        ///       .LoadSave<PlayerData>("slot1")
        ///       .UnwrapOr(new PlayerData());
        /// </summary>
        public static MdixResult<T> LoadSave<T>(
            string  fileName,
            string? prefix = null)
        {
            var path = MdixPaths.SaveFile(fileName);

            if (!File.Exists(path) && !File.Exists(path + ".bak"))
                return MdixError.IoError(
                    $"LoadSave: no save file at '{path}'. " +
                    "Normal on first launch — use UnwrapOr(defaultValue).");

            return ReadFile<T>("LoadSave", path, prefix, requireClosed: true);
        }

        /// <summary>Returns true if a save file with the given name exists.</summary>
        public static bool SaveExists(string fileName) =>
            MdixPaths.SaveExists(fileName);

        /// <summary>
        /// Delete a save file and its backup. Safe to call if the file does not exist.
        /// </summary>
        public static MdixResult<Unit> DeleteSave(string fileName)
        {
            var path = MdixPaths.SaveFile(fileName);
            try
            {
                if (File.Exists(path))          File.Delete(path);
                if (File.Exists(path + ".bak")) File.Delete(path + ".bak");
                if (File.Exists(path + ".tmp")) File.Delete(path + ".tmp");
                return MdixResult<Unit>.Ok(Unit.Value);
            }
            catch (Exception ex)
            {
                return MdixError.IoError(
                    $"DeleteSave: failed to delete '{path}': {ex.Message}", ex);
            }
        }

        // ── Config helpers ────────────────────────────────────────────────────

        /// <summary>
        /// Load a mutable config file from the mdix config directory.
        /// Use this for configs that change at runtime (not bundled read-only data).
        /// </summary>
        public static MdixResult<T> LoadConfig<T>(
            string  fileName,
            string? prefix = null)
        {
            var path = MdixPaths.ConfigFile(fileName);

            if (!File.Exists(path) && !File.Exists(path + ".bak"))
                return MdixError.IoError(
                    $"LoadConfig: no config file at '{path}'.");

            return ReadFile<T>("LoadConfig", path, prefix, requireClosed: false);
        }

        /// <summary>
        /// Save a class or struct to the mdix config directory (written the same safe way as Save).
        /// </summary>
        public static MdixResult<Unit> SaveConfig<T>(
            string  fileName,
            T       data,
            string? prefix = null)
        {
            if (data == null)
                return MdixError.NativeError("SaveConfig: data cannot be null.");

            MdixPaths.EnsureDirectoriesExist();
            return WriteFile("SaveConfig", MdixPaths.ConfigFile(fileName), data, prefix);
        }

        // ── File plumbing ─────────────────────────────────────────────────────

        private static readonly HashSet<string> LoggedReports = new HashSet<string>();

        private static MdixResult<Unit> WriteFile<T>(string who, string path, T data, string? prefix)
        {
            MdixWriteReport report;
            var text = MdixWriter.ToText(data, prefix, out report);
            if (text.IsFailure)
                return MdixResult<Unit>.Err(text.Error);

            if (!report.IsClean)
            {
                var message = $"[Mdix] {who} '{Path.GetFileName(path)}': {report.Describe(5)}";
                bool first;
                lock (LoggedReports) first = LoggedReports.Add(message);
                if (first) Debug.LogWarning(message);
            }

            return WriteAtomic(who, path, text.SuccessResult);
        }

        /// <summary>Writes beside the target, then swaps it in, keeping the old file as .bak.</summary>
        private static MdixResult<Unit> WriteAtomic(string who, string path, string text)
        {
            var tmp = path + ".tmp";
            var bak = path + ".bak";
            try
            {
                File.WriteAllText(tmp, text, new UTF8Encoding(false));

                if (File.Exists(path))
                {
                    try { File.Replace(tmp, path, bak); }
                    catch (Exception)
                    {
                        // Not every file system supports Replace: keep the copy by hand, then swap.
                        File.Copy(path, bak, true);
                        File.Delete(path);
                        File.Move(tmp, path);
                    }
                }
                else
                {
                    File.Move(tmp, path);
                }
                return MdixResult<Unit>.Ok(Unit.Value);
            }
            catch (Exception ex)
            {
                try { if (File.Exists(tmp)) File.Delete(tmp); } catch (Exception) { }
                return MdixError.IoError($"{who}: could not write '{path}': {ex.Message}", ex);
            }
        }

        /// <summary>Reads the file, and when it is missing or damaged, the backup beside it.</summary>
        private static MdixResult<T> ReadFile<T>(string who, string path, string? prefix, bool requireClosed)
        {
            var main = ReadOne<T>(who, path, prefix, requireClosed);
            if (main.IsSuccess) return main;

            var bak = path + ".bak";
            if (File.Exists(bak))
            {
                var restored = ReadOne<T>(who, bak, prefix, requireClosed);
                if (restored.IsSuccess)
                {
                    Debug.LogWarning($"[Mdix] {who}: '{Path.GetFileName(path)}' could not be read " +
                                     $"({main.Error.Message}); the backup from the previous save was used.");
                    return restored;
                }
            }
            return main;
        }

        private static MdixResult<T> ReadOne<T>(string who, string file, string? prefix, bool requireClosed)
        {
            string text;
            try { text = File.ReadAllText(file); }
            catch (Exception ex)
            {
                return MdixError.IoError($"{who}: could not read '{file}': {ex.Message}", ex);
            }

            // The engine accepts a cut-off file and returns what came before the cut, so check the ending.
            if (requireClosed && !text.TrimEnd().EndsWith(")", StringComparison.Ordinal))
                return MdixError.NativeError($"{who}: '{file}' looks cut off (it does not end with a closing bracket).");

            var loaded = Dix.LoadStr(text);
            if (loaded.IsFailure) return MdixResult<T>.Err(loaded.Error);

            using var db = loaded.SuccessResult;
            return MdixBinder.ReadAs<T>(db, MdixBinder.EffectivePrefix(typeof(T), prefix));
        }
    }

    // ── MainThreadDispatcher ──────────────────────────────────────────────────

    /// <summary>
    /// Minimal Unity main-thread dispatcher used internally by
    /// MdixUnityExtensions to marshal async callbacks back to the main thread.
    /// Creates a hidden DontDestroyOnLoad GameObject on first use.
    /// </summary>
    internal sealed class MainThreadDispatcher : MonoBehaviour
    {
        private static MainThreadDispatcher? _instance;

        private static readonly
            System.Collections.Concurrent.ConcurrentQueue<Action>
            _queue = new();

        private static MainThreadDispatcher Instance
        {
            get
            {
                if (_instance != null) return _instance;

                var go = new GameObject("[MdixMainThreadDispatcher]")
                {
                    hideFlags = HideFlags.HideAndDontSave
                };
                DontDestroyOnLoad(go);
                _instance = go.AddComponent<MainThreadDispatcher>();
                return _instance;
            }
        }

        private void Update()
        {
            while (_queue.TryDequeue(out var action))
            {
                try   { action(); }
                catch (Exception ex)
                {
                    Debug.LogError(
                        $"[MdixMainThreadDispatcher] Unhandled exception: {ex}");
                }
            }
        }

        internal static Task RunOnMainThreadAsync(Action action)
        {
            var tcs = new TaskCompletionSource<bool>();
            _ = Instance;

            _queue.Enqueue(() =>
            {
                try
                {
                    action();
                    tcs.SetResult(true);
                }
                catch (Exception ex)
                {
                    tcs.SetException(ex);
                }
            });

            return tcs.Task;
        }
    }
}
