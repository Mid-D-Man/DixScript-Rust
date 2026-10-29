using System;
using System.Globalization;
using System.Reflection;
using UnityEditor;
using UnityEngine;
using UnityEngine.Networking;
using UnityEngine.UIElements;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>
    /// Unity's own editor preview player (UnityEditor.AudioUtil), reached by reflection because it is internal.
    /// If a Unity version does not have it, <see cref="Available"/> is false and nothing else breaks.
    /// </summary>
    internal static class MdixAudioBridge
    {
        private static readonly MethodInfo PlayMethod;
        private static readonly MethodInfo StopMethod;
        private static readonly MethodInfo IsPlayingMethod;

        static MdixAudioBridge()
        {
            var type = typeof(EditorWindow).Assembly.GetType("UnityEditor.AudioUtil");
            if (type == null) return;

            const BindingFlags flags = BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic;
            PlayMethod      = type.GetMethod("PlayPreviewClip",       flags, null, new[] { typeof(AudioClip), typeof(int), typeof(bool) }, null);
            StopMethod      = type.GetMethod("StopAllPreviewClips",   flags, null, Type.EmptyTypes, null);
            IsPlayingMethod = type.GetMethod("IsPreviewClipPlaying",  flags, null, Type.EmptyTypes, null);
        }

        public static bool Available { get { return PlayMethod != null && StopMethod != null && IsPlayingMethod != null; } }

        public static bool Play(AudioClip clip)
        {
            if (!Available) return false;
            try { PlayMethod.Invoke(null, new object[] { clip, 0, false }); return true; }
            catch (Exception) { return false; }
        }

        public static void Stop()
        {
            if (!Available) return;
            try { StopMethod.Invoke(null, null); }
            catch (Exception) { }
        }

        public static bool IsPlaying()
        {
            if (!Available) return false;
            try { return (bool)IsPlayingMethod.Invoke(null, null); }
            catch (Exception) { return false; }
        }
    }

    /// <summary>
    /// A Play / Stop button for a WAV or Ogg blob. The data is written to a temporary file outside the project, read
    /// back through Unity's own decoder (<c>UnityWebRequestMultimedia</c>, polled until it finishes), played with the
    /// editor's preview player, and the temporary file is deleted as soon as it has been read. Nothing is ever
    /// imported into Assets. Decoding happens on the first Play, not when the popup opens.
    /// </summary>
    internal sealed class MdixAudioPreview : IDisposable
    {
        private enum State { Idle, Loading, Ready, Playing, Failed }

        private readonly Func<byte[]>    _readBytes;
        private readonly MdixBlobKind    _kind;
        private readonly VisualElement   _root;
        private readonly Button          _button;
        private readonly Label           _label;

        private State          _state;
        private UnityWebRequest _request;
        private string         _tempFile;
        private AudioClip      _clip;
        private bool           _subscribed;
        private bool           _disposed;

        public VisualElement Root { get { return _root; } }

        public MdixAudioPreview(Func<byte[]> readBytes, MdixBlobKind kind)
        {
            _readBytes = readBytes;
            _kind      = kind;

            _root = new VisualElement();
            _root.AddToClassList("mdix-blob__audio");

            _button = new Button(OnButton) { text = PlayText };
            _button.AddToClassList("mdix-btn");
            _root.Add(_button);

            _label = new Label();
            _label.AddToClassList("mdix-pop__hint");
            _root.Add(_label);

            if (MdixAudioBridge.Available)
            {
                _label.text = "Plays here with Unity's editor preview player.";
            }
            else
            {
                _button.SetEnabled(false);
                _label.text = "Playback is not available in this Unity version. Use Open to play it in another app.";
            }
        }

        private const string PlayText = "▶  Play";
        private const string StopText = "■  Stop";

        // ── Button and states ─────────────────────────────────────────────────

        private void OnButton()
        {
            switch (_state)
            {
                case State.Playing: StopPlaying(); break;
                case State.Ready:   PlayClip();    break;
                case State.Idle:
                case State.Failed:  BeginLoad();   break;
                // Loading: the button is disabled until it finishes.
            }
        }

        private void BeginLoad()
        {
            byte[] bytes;
            try { bytes = _readBytes(); }
            catch (Exception e) { Fail("Could not read the data: " + e.Message); return; }

            if (bytes == null || bytes.Length == 0) { Fail("There is no audio data."); return; }

            try
            {
                _tempFile = MdixBlobTemp.Write(bytes, "audio", MdixBlobInfo.Extension(_kind));

                var type = _kind == MdixBlobKind.Wav ? AudioType.WAV : AudioType.OGGVORBIS;
                _request = UnityWebRequestMultimedia.GetAudioClip(new Uri(_tempFile).AbsoluteUri, type);
                _request.SendWebRequest();
            }
            catch (Exception e)
            {
                Fail("Could not start decoding: " + e.Message);
                return;
            }

            _state = State.Loading;
            _button.SetEnabled(false);
            _label.text = "Decoding…";
            Subscribe();
        }

        // Runs every editor update while something is loading or playing.
        private void OnUpdate()
        {
            if (_disposed) return;

            if (_state == State.Loading)
            {
                if (_request != null && _request.isDone) FinishLoad();
            }
            else if (_state == State.Playing)
            {
                if (!MdixAudioBridge.IsPlaying()) BecomeReady("Finished. Press Play to hear it again.");
            }
        }

        private void FinishLoad()
        {
            string error = null;

            if (_request.result != UnityWebRequest.Result.Success)
            {
                error = _request.error;
            }
            else
            {
                try { _clip = DownloadHandlerAudioClip.GetContent(_request); }
                catch (Exception e) { error = e.Message; }

                if (_clip == null && error == null) error = "Unity could not decode this audio.";
            }

            ReleaseRequest();

            if (error != null) Fail("Unity could not play this audio: " + error);
            else               PlayClip();
        }

        private void PlayClip()
        {
            if (_clip == null) return;

            if (!MdixAudioBridge.Play(_clip))
            {
                Fail("Unity's preview player could not start.");
                return;
            }

            _state = State.Playing;
            _button.text = StopText;
            _button.SetEnabled(true);
            _label.text = "Playing  ·  " + _clip.length.ToString("0.0", CultureInfo.InvariantCulture) + " s";
            Subscribe();
        }

        private void StopPlaying()
        {
            MdixAudioBridge.Stop();
            BecomeReady("Stopped.");
        }

        private void BecomeReady(string message)
        {
            _state = State.Ready;
            _button.text = PlayText;
            _button.SetEnabled(true);
            _label.text = message;
            Unsubscribe();
        }

        private void Fail(string message)
        {
            ReleaseRequest();

            _state = State.Failed;
            _button.text = PlayText;
            _button.SetEnabled(MdixAudioBridge.Available);
            _label.text = message;
            Unsubscribe();
        }

        // ── Cleanup ───────────────────────────────────────────────────────────

        // Ends a request, in flight or finished, and deletes the temporary file it read.
        private void ReleaseRequest()
        {
            if (_request != null)
            {
                if (!_request.isDone) _request.Abort();
                _request.Dispose();
                _request = null;
            }

            MdixBlobTemp.Delete(_tempFile);
            _tempFile = null;
        }

        private void Subscribe()
        {
            if (_subscribed) return;
            _subscribed = true;
            EditorApplication.update += OnUpdate;
        }

        private void Unsubscribe()
        {
            if (!_subscribed) return;
            _subscribed = false;
            EditorApplication.update -= OnUpdate;
        }

        /// <summary>Stops playback, ends any decoding, deletes the temporary file and frees the clip.</summary>
        public void Dispose()
        {
            if (_disposed) return;
            _disposed = true;

            Unsubscribe();
            if (_state == State.Playing) MdixAudioBridge.Stop();
            ReleaseRequest();

            if (_clip != null)
            {
                UnityEngine.Object.DestroyImmediate(_clip);
                _clip = null;
            }
        }
    }
}
