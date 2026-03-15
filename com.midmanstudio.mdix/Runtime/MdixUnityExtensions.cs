using System;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using System.Collections;
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
        /// Deserialize a MdixAsset directly into a POCO of type T.
        /// No database handle to manage.
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
            onComplete(result!.Value);
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
        /// Serialize a POCO and save it to the mdix saves directory.
        /// Directories are created automatically.
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

            using var builder = MdixBuilder.Create();

            var serResult = builder.Serialize(data, prefix);
            if (serResult.IsFailure) return serResult;

            return builder.Save(MdixPaths.SaveFile(fileName));
        }

        /// <summary>
        /// Load a POCO from a save file in the mdix saves directory.
        /// Returns a failed result if the file does not exist —
        /// this is normal on first launch, not an error.
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

            if (!File.Exists(path))
                return MdixError.IoError(
                    $"LoadSave: no save file at '{path}'. " +
                    "Normal on first launch — use UnwrapOr(defaultValue).");

            return Dix.Deserialize<T>(path, prefix);
        }

        /// <summary>Returns true if a save file with the given name exists.</summary>
        public static bool SaveExists(string fileName) =>
            MdixPaths.SaveExists(fileName);

        /// <summary>
        /// Delete a save file. Safe to call if the file does not exist.
        /// </summary>
        public static MdixResult<Unit> DeleteSave(string fileName)
        {
            var path = MdixPaths.SaveFile(fileName);
            try
            {
                if (File.Exists(path)) File.Delete(path);
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

            if (!File.Exists(path))
                return MdixError.IoError(
                    $"LoadConfig: no config file at '{path}'.");

            return Dix.Deserialize<T>(path, prefix);
        }

        /// <summary>
        /// Save a POCO to the mdix config directory.
        /// </summary>
        public static MdixResult<Unit> SaveConfig<T>(
            string  fileName,
            T       data,
            string? prefix = null)
        {
            if (data == null)
                return MdixError.NativeError("SaveConfig: data cannot be null.");

            MdixPaths.EnsureDirectoriesExist();

            using var builder = MdixBuilder.Create();
            var serResult = builder.Serialize(data, prefix);
            if (serResult.IsFailure) return serResult;

            return builder.Save(MdixPaths.ConfigFile(fileName));
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
