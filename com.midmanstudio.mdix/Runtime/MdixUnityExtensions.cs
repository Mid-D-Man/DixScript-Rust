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
    /// Handles coroutine loading, platform save paths, and main-thread
    /// callback dispatch for async operations.
    /// </summary>
    public static class MdixUnityExtensions
    {
        // ── Save / persistent data paths ─────────────────────────────────────

        /// <summary>
        /// Full path for a save file in Unity's persistent data directory.
        /// This is the correct location for player save data on all platforms.
        ///
        /// Example: MdixSavePath("savegame") →
        ///   Android: /data/data/com.company.game/files/savegame.mdix
        ///   iOS:     .../Documents/savegame.mdix
        ///   PC:      %APPDATA%/CompanyName/GameName/savegame.mdix
        /// </summary>
        public static string MdixSavePath(string fileName)
        {
            if (string.IsNullOrEmpty(fileName))
                throw new ArgumentNullException(nameof(fileName));

            if (!fileName.EndsWith(".mdix", StringComparison.OrdinalIgnoreCase))
                fileName += ".mdix";

            return Path.Combine(Application.persistentDataPath, fileName);
        }

        /// <summary>
        /// Full path for an encrypted save file in Unity's persistent data directory.
        /// </summary>
        public static string MdixEncSavePath(string fileName)
        {
            if (string.IsNullOrEmpty(fileName))
                throw new ArgumentNullException(nameof(fileName));

            var baseName = Path.GetFileNameWithoutExtension(fileName);
            return Path.Combine(Application.persistentDataPath, baseName + ".mdix.enc");
        }

        /// <summary>
        /// Full path to a streaming asset .mdix file.
        /// StreamingAssets is read-only at runtime on mobile — use this for
        /// bundled read-only game data (enemy tables, item definitions, etc.).
        /// For save data, use MdixSavePath() instead.
        /// </summary>
        public static string MdixStreamingPath(string relativePath)
        {
            if (string.IsNullOrEmpty(relativePath))
                throw new ArgumentNullException(nameof(relativePath));

            return Path.Combine(Application.streamingAssetsPath, relativePath);
        }

        // ── MdixAsset load helpers ────────────────────────────────────────────

        /// <summary>
        /// Load a MdixDatabase from a MdixAsset reference.
        /// The caller must dispose the returned database.
        ///
        /// Returns a failed result if the asset is null or has no source data.
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
        public static MdixResult<T> LoadAs<T>(this MdixAsset asset, string? prefix = null)
        {
            if (asset == null)
                return MdixError.NativeError(
                    "LoadAs: asset reference is null.");

            return asset.LoadAs<T>(prefix);
        }

        // ── Coroutine loading ─────────────────────────────────────────────────

        /// <summary>
        /// Load a .mdix file via coroutine.
        /// Useful for loading from StreamingAssets on Android where file access
        /// requires UnityWebRequest rather than direct File.Read.
        ///
        /// Usage:
        ///   yield return MdixUnityExtensions.LoadCoroutine(
        ///       path, result => { using var db = result.OrThrow(); ... });
        /// </summary>
        public static IEnumerator LoadCoroutine(
            string                          path,
            Action<MdixResult<MdixDatabase>> onComplete)
        {
            if (onComplete == null) throw new ArgumentNullException(nameof(onComplete));

            MdixResult<MdixDatabase>? result = null;

#if UNITY_ANDROID && !UNITY_EDITOR
            // On Android StreamingAssets lives inside the APK — use
            // UnityWebRequest to read it, then parse the string.
            var www = UnityEngine.Networking.UnityWebRequest.Get(path);
            yield return www.SendWebRequest();

            if (www.result != UnityEngine.Networking.UnityWebRequest.Result.Success)
            {
                result = MdixError.IoError(
                    $"LoadCoroutine: UnityWebRequest failed: {www.error}");
            }
            else
            {
                result = Dix.LoadStr(www.downloadHandler.text);
            }
            www.Dispose();
#else
            // All other platforms support direct file access.
            // Run the blocking IO on a thread pool thread, yield until done.
            var task = Task.Run(() => Dix.Load(path));

            while (!task.IsCompleted)
                yield return null;

            result = task.Result;
#endif
            onComplete(result!.Value);
        }

        // ── Async with main-thread callback ───────────────────────────────────

        /// <summary>
        /// Asynchronously load a .mdix file, then invoke a callback on
        /// the Unity main thread when complete.
        ///
        /// Safe to call from MonoBehaviour.Start() or any async Unity method.
        /// The callback runs on the main thread — safe to use UnityEngine APIs.
        ///
        /// Usage:
        ///   await MdixUnityExtensions.LoadAsync(path, result =>
        ///   {
        ///       using var db = result.OrThrow();
        ///       healthText.text = db.GetInt("player.health").UnwrapOr(0).ToString();
        ///   });
        /// </summary>
        public static async Task LoadAsync(
            string                          path,
            Action<MdixResult<MdixDatabase>> onComplete,
            CancellationToken               ct = default)
        {
            if (onComplete == null) throw new ArgumentNullException(nameof(onComplete));

            var result = await Dix.LoadAsync(path, ct).ConfigureAwait(false);

            // Marshal back to main thread.
            await MainThreadDispatcher.RunOnMainThreadAsync(() => onComplete(result));
        }

        // ── Quick save helpers ────────────────────────────────────────────────

        /// <summary>
        /// Save a POCO object to a .mdix file in persistentDataPath.
        ///
        /// Creates the file if it does not exist.
        /// This is the simplest save-game pattern — call at checkpoint,
        /// game over, or periodic autosave.
        ///
        /// Example:
        ///   MdixUnityExtensions.Save("savegame", playerData);
        /// </summary>
        public static MdixResult<Unit> Save<T>(string fileName, T data, string? prefix = null)
        {
            if (data == null)
                return MdixError.NativeError("Save: data cannot be null.");

            using var builder = MdixBuilder.Create();

            var serResult = builder.Serialize(data, prefix);
            if (serResult.IsFailure)
                return serResult;

            return builder.Save(MdixSavePath(fileName));
        }

        /// <summary>
        /// Load a POCO object from a .mdix save file in persistentDataPath.
        /// Returns a failed result if the file does not exist.
        ///
        /// Example:
        ///   var result = MdixUnityExtensions.LoadSave<PlayerData>("savegame");
        ///   var player = result.UnwrapOr(new PlayerData());
        /// </summary>
        public static MdixResult<T> LoadSave<T>(string fileName, string? prefix = null)
        {
            var path = MdixSavePath(fileName);

            if (!File.Exists(path))
                return MdixError.IoError(
                    $"LoadSave: no save file found at '{path}'. " +
                    "This is normal on first launch.");

            return Dix.Deserialize<T>(path, prefix);
        }

        /// <summary>
        /// Returns true if a save file with the given name exists in persistentDataPath.
        /// </summary>
        public static bool SaveExists(string fileName)
        {
            return File.Exists(MdixSavePath(fileName));
        }

        /// <summary>
        /// Delete a save file from persistentDataPath.
        /// Safe to call even if the file does not exist.
        /// </summary>
        public static MdixResult<Unit> DeleteSave(string fileName)
        {
            var path = MdixSavePath(fileName);
            try
            {
                if (File.Exists(path))
                    File.Delete(path);
                return MdixResult<Unit>.Ok(Unit.Value);
            }
            catch (Exception ex)
            {
                return MdixError.IoError(
                    $"DeleteSave: failed to delete '{path}': {ex.Message}", ex);
            }
        }
    }

    // ── MainThreadDispatcher ──────────────────────────────────────────────────

    /// <summary>
    /// Minimal Unity main-thread dispatcher.
    /// Used internally by MdixUnityExtensions to marshal async callbacks
    /// back to the main thread.
    ///
    /// Automatically creates a hidden GameObject on first use.
    /// The GameObject persists across scene loads (DontDestroyOnLoad).
    /// </summary>
    internal sealed class MainThreadDispatcher : MonoBehaviour
    {
        private static MainThreadDispatcher? _instance;
        private static readonly System.Collections.Concurrent.ConcurrentQueue<Action>
            _queue = new System.Collections.Concurrent.ConcurrentQueue<Action>();

        private static MainThreadDispatcher Instance
        {
            get
            {
                if (_instance == null)
                {
                    var go = new GameObject("[MdixMainThreadDispatcher]")
                    {
                        hideFlags = HideFlags.HideAndDontSave
                    };
                    DontDestroyOnLoad(go);
                    _instance = go.AddComponent<MainThreadDispatcher>();
                }
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
                    Debug.LogError($"[MdixMainThreadDispatcher] Unhandled exception: {ex}");
                }
            }
        }

        internal static Task RunOnMainThreadAsync(Action action)
        {
            var tcs = new TaskCompletionSource<bool>();

            _ = Instance; // ensure the GameObject exists

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
