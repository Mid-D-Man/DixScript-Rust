using UnityEngine;

namespace MidManStudio.Mdix.Unity
{
    /// <summary>
    /// Runs automatically before the first scene loads on every platform.
    /// Creates the mdix directory structure under persistentDataPath if it
    /// does not exist yet — developers do not need to call this manually.
    /// </summary>
    internal static class MdixInitializer
    {
        [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.BeforeSceneLoad)]
        private static void Initialize()
        {
            try
            {
                MdixPaths.EnsureDirectoriesExist();
            }
            catch (System.Exception ex)
            {
                // Non-fatal — log and continue. The paths will be created
                // on demand by whichever helper is called first.
                Debug.LogWarning(
                    $"[MDIX] Failed to create data directories on startup: {ex.Message}\n" +
                    $"Expected location: {MdixPaths.Root}");
            }
        }
    }
}
