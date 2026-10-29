using System;
using System.IO;
using UnityEditor;
using UnityEngine;
using UnityEngine.UIElements;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>
    /// The content of the blob popup: what the data is, a preview that suits it, and four actions.
    ///
    /// Preview: a PNG or JPEG is shown as an image, a WAV or Ogg can be played, text is shown as text, and anything
    /// else gets the first bytes as a hex dump. Only the start of the data is read for types that need only that.
    ///
    /// Actions: <b>Replace from file</b> and <b>Clear</b> change the blob in this popup only; the preview shows the new
    /// data and <see cref="IsChanged"/> turns true, but nothing reaches the file until the popup's Apply. <b>Save as</b>
    /// writes the data to a file, and <b>Open</b> opens a temporary copy in the default app. A file bigger than
    /// <see cref="MdixBlobInfo.MaxReplaceBytes"/> is refused, because it would be written into the text as base64.
    /// </summary>
    internal sealed class MdixBlobEditor
    {
        private const int PreviewMaxWidth  = 400;
        private const int PreviewMaxHeight = 240;

        private static string _lastDirectory = string.Empty;       // where the last file dialog was, for this session

        private readonly string        _path;
        private readonly string        _initial;
        private readonly VisualElement _root;
        private readonly VisualElement _preview;
        private readonly Label         _info;
        private readonly Label         _notice;
        private readonly Button        _save;
        private readonly Button        _open;
        private readonly Button        _clear;

        private string          _current;
        private MdixBlobKind    _kind;
        private Texture2D       _texture;
        private MdixAudioPreview _audio;

        public VisualElement Root { get { return _root; } }

        /// <summary>The blob as base64: the original until the person replaces or clears it.</summary>
        public string Base64 { get { return _current; } }

        /// <summary>True once the blob differs from what the file has.</summary>
        public bool IsChanged { get { return !string.Equals(_current, _initial, StringComparison.Ordinal); } }

        /// <summary>Raised when Replace or Clear changed the blob, so the popup can enable Apply.</summary>
        public event Action Changed;

        public MdixBlobEditor(string path, string initialBase64)
        {
            _path    = path ?? string.Empty;
            _initial = initialBase64 ?? string.Empty;
            _current = _initial;

            MdixBlobTemp.CleanOld(TimeSpan.FromDays(1));      // copies handed to other apps earlier

            _root = new VisualElement();
            _root.AddToClassList("mdix-blob");

            _info = new Label();
            _info.AddToClassList("mdix-blob__info");
            _root.Add(_info);

            _preview = new VisualElement();
            _preview.AddToClassList("mdix-blob__preview");
            _root.Add(_preview);

            _notice = new Label();
            _notice.style.display = DisplayStyle.None;
            _root.Add(_notice);

            var buttons = new VisualElement();
            buttons.AddToClassList("mdix-blob__buttons");

            buttons.Add(MakeButton("Replace from file…", "Choose a file to put in this blob (up to 1 MB).", OnReplace));
            _save  = MakeButton("Save as…", "Write the data to a file.", OnSave);
            _open  = MakeButton("Open",     "Open a temporary copy in the default app.", OnOpen);
            _clear = MakeButton("Clear",    "Make the blob empty.", OnClear);
            buttons.Add(_save);
            buttons.Add(_open);
            buttons.Add(_clear);
            _root.Add(buttons);

            Rebuild();
        }

        /// <summary>Stops playback and frees the preview. Safe to call twice.</summary>
        public void Close()
        {
            DisposePreview();
        }

        // ── Showing the data ──────────────────────────────────────────────────

        private void Rebuild()
        {
            DisposePreview();
            _preview.Clear();

            var size = MdixBlobInfo.DecodedLength(_current);

            _clear.SetEnabled(_current.Length > 0);
            _save.SetEnabled(size > 0);
            _open.SetEnabled(size > 0);

            if (size < 0)
            {
                _info.text = "The data is not valid base64.";
                _kind = MdixBlobKind.Unknown;
                return;
            }

            if (size == 0)
            {
                _kind = MdixBlobKind.Unknown;
                _info.text = "Empty blob" + ChangedNote();
                AddHint("No data yet. Use Replace from file to fill it.");
                return;
            }

            var head = MdixBlobInfo.HeadBytes(_current, MdixBlobInfo.SniffBytes);
            _kind    = MdixBlobInfo.Sniff(head, head.Length);
            _info.text = MdixBlobInfo.LongLabel(_kind) + "  ·  " + MdixBlobInfo.FormatSize(size) + ChangedNote();

            if (size > MdixBlobInfo.MaxPreviewBytes)
            {
                AddHint("Too large to preview (over " + MdixBlobInfo.FormatSize(MdixBlobInfo.MaxPreviewBytes) + "). You can still save it to a file.");
                return;
            }

            if      (MdixBlobInfo.CanPreviewImage(_kind)) ShowImage(head, size);
            else if (MdixBlobInfo.CanPlay(_kind))         ShowAudio();
            else if (_kind == MdixBlobKind.Text)          ShowText(size);
            else                                          ShowOther(head, size);
        }

        private string ChangedNote()
        {
            return IsChanged ? "  ·  changed, not applied yet" : string.Empty;
        }

        private void ShowImage(byte[] head, long size)
        {
            byte[] bytes;
            if (!MdixBlobInfo.TryDecode(_current, out bytes)) { AddHint("The data could not be read."); return; }

            var texture = new Texture2D(2, 2, TextureFormat.RGBA32, false) { hideFlags = HideFlags.HideAndDontSave };
            if (!texture.LoadImage(bytes))
            {
                UnityEngine.Object.DestroyImmediate(texture);
                AddHint("Unity could not read this image.");
                AddDump(MdixBlobInfo.HexDump(head, size));
                return;
            }

            _texture = texture;

            int w, h;
            MdixBlobInfo.FitSize(texture.width, texture.height, PreviewMaxWidth, PreviewMaxHeight, out w, out h);

            var image = new Image { image = texture, scaleMode = ScaleMode.ScaleToFit };
            image.style.width  = w;
            image.style.height = h;
            _preview.Add(image);

            AddHint(texture.width + " × " + texture.height + " px");
        }

        private void ShowAudio()
        {
            var snapshot = _current;
            _audio = new MdixAudioPreview(() =>
            {
                byte[] bytes;
                return MdixBlobInfo.TryDecode(snapshot, out bytes) ? bytes : null;
            }, _kind);
            _preview.Add(_audio.Root);
        }

        private void ShowText(long size)
        {
            var head = MdixBlobInfo.HeadBytes(_current, MdixBlobInfo.TextPreviewBytes);

            bool truncated;
            var text = MdixBlobInfo.TextPreview(head, size, out truncated);

            AddDump(text);
            if (truncated) AddHint("Only the start is shown.");
        }

        private void ShowOther(byte[] head, long size)
        {
            AddHint(NoPreviewReason());
            AddDump(MdixBlobInfo.HexDump(head, size));
        }

        private string NoPreviewReason()
        {
            switch (_kind)
            {
                case MdixBlobKind.Gif:
                case MdixBlobKind.Bmp:
                case MdixBlobKind.Webp:
                    return "Unity 2022.3 cannot show this image type here. Use Open to see it in another app, or Save as. The first bytes:";

                case MdixBlobKind.Mp3:
                case MdixBlobKind.Flac:
                case MdixBlobKind.M4a:
                case MdixBlobKind.WebM:
                case MdixBlobKind.Mp4:
                    return "Unity's editor cannot play this type here. Use Open to play it in another app, or Save as. The first bytes:";

                default:
                    return "No preview for this type. The first bytes:";
            }
        }

        private void AddHint(string text)
        {
            if (string.IsNullOrEmpty(text)) return;

            var label = new Label(text);
            label.AddToClassList("mdix-pop__hint");
            _preview.Add(label);
        }

        private void AddDump(string text)
        {
            var scroll = new ScrollView(ScrollViewMode.VerticalAndHorizontal);
            scroll.AddToClassList("mdix-blob__dump");

            var label = new Label(text);
            label.AddToClassList("mdix-blob__mono");
            scroll.Add(label);

            _preview.Add(scroll);
        }

        private void DisposePreview()
        {
            if (_audio != null)
            {
                _audio.Dispose();
                _audio = null;
            }

            if (_texture != null)
            {
                UnityEngine.Object.DestroyImmediate(_texture);
                _texture = null;
            }
        }

        // ── Actions ───────────────────────────────────────────────────────────

        private void OnReplace()
        {
            var file = EditorUtility.OpenFilePanel("Replace the blob with a file", _lastDirectory, string.Empty);
            if (string.IsNullOrEmpty(file)) return;

            try
            {
                _lastDirectory = Path.GetDirectoryName(file) ?? string.Empty;

                var length = new FileInfo(file).Length;
                if (length > MdixBlobInfo.MaxReplaceBytes)
                {
                    SetNotice(Path.GetFileName(file) + " is " + MdixBlobInfo.FormatSize(length) + ". A blob is limited to " +
                              MdixBlobInfo.FormatSize(MdixBlobInfo.MaxReplaceBytes) + ", because it is written into the text as base64.", true);
                    return;
                }

                Stage(Convert.ToBase64String(File.ReadAllBytes(file)), "Loaded " + Path.GetFileName(file) + ". Apply to write it into the file.");
            }
            catch (Exception e)
            {
                SetNotice("Could not read the file: " + e.Message, true);
            }
        }

        private void OnClear()
        {
            Stage(string.Empty, "Cleared. Apply to write that into the file.");
        }

        private void OnSave()
        {
            byte[] bytes;
            if (!MdixBlobInfo.TryDecode(_current, out bytes)) { SetNotice("The data is not valid base64.", true); return; }

            var suggested = MdixBlobInfo.SuggestedFileName(_path, _kind);
            var extension = MdixBlobInfo.Extension(_kind).TrimStart('.');

            var file = EditorUtility.SaveFilePanel("Save the blob as", _lastDirectory, suggested, extension);
            if (string.IsNullOrEmpty(file)) return;

            try
            {
                _lastDirectory = Path.GetDirectoryName(file) ?? string.Empty;
                File.WriteAllBytes(file, bytes);
                SetNotice("Saved " + file, false);
            }
            catch (Exception e)
            {
                SetNotice("Could not save the file: " + e.Message, true);
            }
        }

        private void OnOpen()
        {
            byte[] bytes;
            if (!MdixBlobInfo.TryDecode(_current, out bytes)) { SetNotice("The data is not valid base64.", true); return; }

            try
            {
                var file = MdixBlobTemp.Write(bytes, "blob", MdixBlobInfo.Extension(_kind));
                EditorUtility.OpenWithDefaultApp(file);
                SetNotice("Opened a temporary copy in the default app. Changes made there are not read back.", false);
            }
            catch (Exception e)
            {
                SetNotice("Could not open the file: " + e.Message, true);
            }
        }

        // Puts new data in this popup (not in the file) and shows it.
        private void Stage(string base64, string message)
        {
            _current = base64;
            SetNotice(message, false);
            Rebuild();

            var handler = Changed;
            if (handler != null) handler();
        }

        private void SetNotice(string text, bool error)
        {
            _notice.text = text;
            _notice.style.display = string.IsNullOrEmpty(text) ? DisplayStyle.None : DisplayStyle.Flex;
            _notice.EnableInClassList("mdix-pop__error", error);
            _notice.EnableInClassList("mdix-pop__hint", !error);
        }

        private static Button MakeButton(string text, string tooltip, Action click)
        {
            var button = new Button(click) { text = text, tooltip = tooltip };
            button.AddToClassList("mdix-btn");
            return button;
        }
    }
}
