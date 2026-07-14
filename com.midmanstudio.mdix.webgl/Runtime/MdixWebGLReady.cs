using System.Collections;
using System.Runtime.InteropServices;
using UnityEngine;

namespace MidManStudio.Mdix.WebGL
{
    /// <summary>
    /// Mdix's native backend (mdix-ffi) cannot be linked into a WebGL player, so
    /// on this platform MdixDatabase's calls are bridged to mdix-wasm through
    /// MdixWeb.jslib instead (see that file for the full "why" and "how").
    /// That bridge needs the mdix-wasm module to finish an async load before any
    /// Dix.LoadStr() call can succeed — this class is how you wait for that.
    ///
    /// REQUIRED SETUP: this alone does nothing. Za WebGL template's index.html
    /// must load mdix-bootstrap.js (see Runtime/WebGLTemplate/mdix-bootstrap.js
    /// in this package) before the Unity loader script runs. Without it,
    /// IsReady never becomes true and WaitUntilReady() will time out.
    ///
    /// Usage:
    /// <code>
    /// IEnumerator Start()
    /// {
    ///     yield return MdixWebGLReady.WaitUntilReady();
    ///     if (!MdixWebGLReady.IsReady) yield break; // timed out, already logged
    ///
    ///     var result = Dix.LoadStr(mySource);
    ///     // ...
    /// }
    /// </code>
    /// </summary>
    public static class MdixWebGLReady
    {
#if UNITY_WEBGL && !UNITY_EDITOR
        [DllImport("__Internal")]
        private static extern int mdix_web_is_ready();
#endif

        /// <summary>
        /// True once the browser has finished loading mdix-wasm and it's safe to
        /// call Dix.LoadStr / MdixDatabase.LoadStr. Always true off WebGL (and in
        /// the Editor, where the browser bridge never actually runs) so shared
        /// code can check this unconditionally without extra platform checks.
        /// </summary>
        public static bool IsReady
        {
            get
            {
#if UNITY_WEBGL && !UNITY_EDITOR
                return mdix_web_is_ready() != 0;
#else
                return true;
#endif
            }
        }

        /// <summary>
        /// Polls until IsReady or timeoutSeconds elapses. Safe to yield on from
        /// multiple places concurrently. Logs an actionable error and returns
        /// (does not throw) on timeout — check IsReady afterward if you need to
        /// branch on success.
        /// </summary>
        public static IEnumerator WaitUntilReady(float timeoutSeconds = 15f)
        {
            float start = Time.realtimeSinceStartup;
            while (!IsReady)
            {
                if (Time.realtimeSinceStartup - start > timeoutSeconds)
                {
                    Debug.LogError(
                        "[MDIX][WebGL] Timed out after " + timeoutSeconds +
                        "s waiting for the mdix-wasm bridge to become ready. " +
                        "Check that mdix-bootstrap.js is loaded from your WebGL " +
                        "template's <head> (before the Unity loader script) and " +
                        "that the mdix-wasm-web/ files it imports were actually " +
                        "copied alongside it. See this package's README.md.");
                    yield break;
                }
                yield return null;
            }
        }
    }

    /// <summary>
    /// Starts polling for readiness as early as possible (before the first
    /// scene loads) purely so the timeout-error console message in
    /// MdixWebGLReady.WaitUntilReady above surfaces early if the bootstrap
    /// script was never wired up — not required for correctness, since
    /// WaitUntilReady() works fine even if nothing else in the app ever
    /// starts this coroutine.
    /// </summary>
    internal static class MdixWebGLBootstrap
    {
#if UNITY_WEBGL && !UNITY_EDITOR
        [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.BeforeSceneLoad)]
        private static void Initialize()
        {
            var go = new GameObject("[MDIX WebGL Bootstrap]")
            {
                hideFlags = HideFlags.HideInHierarchy
            };
            Object.DontDestroyOnLoad(go);
            go.AddComponent<MdixWebGLBootstrapRunner>();
        }
#endif
    }

#if UNITY_WEBGL && !UNITY_EDITOR
    internal sealed class MdixWebGLBootstrapRunner : MonoBehaviour
    {
        private void Awake() => StartCoroutine(MdixWebGLReady.WaitUntilReady());
    }
#endif
}
