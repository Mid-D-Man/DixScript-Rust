using System;
using System.Globalization;
using System.Text;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>What a blob's first bytes say it is.</summary>
    internal enum MdixBlobKind { Unknown, Png, Jpeg, Gif, Bmp, Webp, Wav, Flac, Ogg, Mp3, WebM, Mp4, M4a, Pdf, Text }

    /// <summary>
    /// Describes a <c>b:("base64...")</c> value without Unity types, so it can be tested on its own.
    ///
    /// The type is found from the first bytes only (magic numbers, then a printable-text test), the same way
    /// the VS Code extension's blob preview does it. Only the start of the base64 is decoded, so a large
    /// blob costs nothing to describe.
    /// </summary>
    internal static class MdixBlobInfo
    {
        /// <summary>How many leading bytes are enough to tell the type.</summary>
        public const int SniffBytes = 512;

        /// <summary>A blob bigger than this is not decoded for a preview (the data is still there to save).</summary>
        public const int MaxPreviewBytes = 8 * 1024 * 1024;

        /// <summary>
        /// The biggest file that can be put into a blob. It is written into the .mdix text as base64 (a third larger),
        /// and the editor tab shows that text, so a big file would make the text unwieldy.
        /// </summary>
        public const int MaxReplaceBytes = 1024 * 1024;

        /// <summary>How much of a blob is read for a text preview, and how many characters of it are shown.</summary>
        public const int TextPreviewBytes = 8192;
        public const int TextPreviewChars = 4000;

        /// <summary>How many bytes a hex dump shows.</summary>
        public const int HexDumpBytes = 256;

        private static readonly CultureInfo Inv = CultureInfo.InvariantCulture;

        // ── Decoding ──────────────────────────────────────────────────────────

        /// <summary>The first <paramref name="maxBytes"/> bytes of the data; empty when the base64 cannot be read.</summary>
        public static byte[] HeadBytes(string base64, int maxBytes)
        {
            if (string.IsNullOrEmpty(base64) || maxBytes <= 0) return Array.Empty<byte>();

            var chars = Math.Min(base64.Length, (maxBytes + 2) / 3 * 4);
            if (chars < base64.Length) chars -= chars % 4;           // cut on a group of four so the piece decodes alone
            if (chars <= 0) return Array.Empty<byte>();

            try
            {
                var bytes = Convert.FromBase64String(base64.Substring(0, chars));
                if (bytes.Length > maxBytes) Array.Resize(ref bytes, maxBytes);
                return bytes;
            }
            catch (FormatException)
            {
                return Array.Empty<byte>();
            }
        }

        /// <summary>The number of bytes the base64 decodes to, without decoding it; -1 when it cannot be right.</summary>
        public static long DecodedLength(string base64)
        {
            if (string.IsNullOrEmpty(base64)) return 0;
            if (base64.Length % 4 != 0) return -1;

            var padding = base64.EndsWith("==", StringComparison.Ordinal) ? 2
                        : base64.EndsWith("=",  StringComparison.Ordinal) ? 1
                        : 0;
            return (long)base64.Length / 4 * 3 - padding;
        }

        // ── Sniffing ──────────────────────────────────────────────────────────

        public static MdixBlobKind Sniff(byte[] b, int count)
        {
            if (b == null) return MdixBlobKind.Unknown;
            if (count > b.Length) count = b.Length;

            if (StartsWith(b, count, 0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A)) return MdixBlobKind.Png;
            if (StartsWith(b, count, 0xFF, 0xD8, 0xFF))                               return MdixBlobKind.Jpeg;
            if (Ascii(b, count, 0, "GIF87a") || Ascii(b, count, 0, "GIF89a"))         return MdixBlobKind.Gif;

            // "BM" alone is not enough (plain text can start with it): a BMP also has its two reserved header
            // fields, bytes 6 to 9, set to zero.
            if (StartsWith(b, count, 0x42, 0x4D) && count >= 14 &&
                b[6] == 0 && b[7] == 0 && b[8] == 0 && b[9] == 0)                      return MdixBlobKind.Bmp;

            if (Ascii(b, count, 0, "RIFF") && Ascii(b, count, 8, "WEBP"))             return MdixBlobKind.Webp;
            if (Ascii(b, count, 0, "RIFF") && Ascii(b, count, 8, "WAVE"))             return MdixBlobKind.Wav;
            if (Ascii(b, count, 0, "fLaC"))                                           return MdixBlobKind.Flac;
            if (Ascii(b, count, 0, "OggS"))                                           return MdixBlobKind.Ogg;
            if (Ascii(b, count, 0, "ID3") || (count > 1 && b[0] == 0xFF && (b[1] & 0xE0) == 0xE0))
                                                                                       return MdixBlobKind.Mp3;
            if (StartsWith(b, count, 0x1A, 0x45, 0xDF, 0xA3))                         return MdixBlobKind.WebM;

            if (Ascii(b, count, 4, "ftyp"))
                return Ascii(b, count, 8, "M4A") ? MdixBlobKind.M4a : MdixBlobKind.Mp4;

            if (Ascii(b, count, 0, "%PDF"))                                           return MdixBlobKind.Pdf;

            // Printable-text test on the first 512 bytes: at least 95% printable ASCII or common whitespace.
            var sample = Math.Min(SniffBytes, count);
            if (sample > 0)
            {
                var printable = 0;
                for (int i = 0; i < sample; i++)
                {
                    var v = b[i];
                    if ((v >= 0x20 && v <= 0x7E) || v == 0x09 || v == 0x0A || v == 0x0D) printable++;
                }
                if ((double)printable / sample > 0.95) return MdixBlobKind.Text;
            }

            return MdixBlobKind.Unknown;
        }

        public static string ShortLabel(MdixBlobKind kind)
        {
            switch (kind)
            {
                case MdixBlobKind.Png:  return "PNG";
                case MdixBlobKind.Jpeg: return "JPEG";
                case MdixBlobKind.Gif:  return "GIF";
                case MdixBlobKind.Bmp:  return "BMP";
                case MdixBlobKind.Webp: return "WebP";
                case MdixBlobKind.Wav:  return "WAV";
                case MdixBlobKind.Flac: return "FLAC";
                case MdixBlobKind.Ogg:  return "OGG";
                case MdixBlobKind.Mp3:  return "MP3";
                case MdixBlobKind.WebM: return "WebM";
                case MdixBlobKind.Mp4:  return "MP4";
                case MdixBlobKind.M4a:  return "M4A";
                case MdixBlobKind.Pdf:  return "PDF";
                case MdixBlobKind.Text: return "text";
                default:                return "binary";
            }
        }

        // ── What a type means ─────────────────────────────────────────────────

        /// <summary>For example "PNG image".</summary>
        public static string LongLabel(MdixBlobKind kind)
        {
            switch (kind)
            {
                case MdixBlobKind.Png:  return "PNG image";
                case MdixBlobKind.Jpeg: return "JPEG image";
                case MdixBlobKind.Gif:  return "GIF image";
                case MdixBlobKind.Bmp:  return "BMP image";
                case MdixBlobKind.Webp: return "WebP image";
                case MdixBlobKind.Wav:  return "WAV audio";
                case MdixBlobKind.Flac: return "FLAC audio";
                case MdixBlobKind.Ogg:  return "Ogg audio";
                case MdixBlobKind.Mp3:  return "MP3 audio";
                case MdixBlobKind.WebM: return "WebM video";
                case MdixBlobKind.Mp4:  return "MP4 video";
                case MdixBlobKind.M4a:  return "M4A audio";
                case MdixBlobKind.Pdf:  return "PDF document";
                case MdixBlobKind.Text: return "Text";
                default:                return "Binary data";
            }
        }

        /// <summary>The file extension for a type, with the dot (".bin" when unknown).</summary>
        public static string Extension(MdixBlobKind kind)
        {
            switch (kind)
            {
                case MdixBlobKind.Png:  return ".png";
                case MdixBlobKind.Jpeg: return ".jpg";
                case MdixBlobKind.Gif:  return ".gif";
                case MdixBlobKind.Bmp:  return ".bmp";
                case MdixBlobKind.Webp: return ".webp";
                case MdixBlobKind.Wav:  return ".wav";
                case MdixBlobKind.Flac: return ".flac";
                case MdixBlobKind.Ogg:  return ".ogg";
                case MdixBlobKind.Mp3:  return ".mp3";
                case MdixBlobKind.WebM: return ".webm";
                case MdixBlobKind.Mp4:  return ".mp4";
                case MdixBlobKind.M4a:  return ".m4a";
                case MdixBlobKind.Pdf:  return ".pdf";
                case MdixBlobKind.Text: return ".txt";
                default:                return ".bin";
            }
        }

        /// <summary>Unity 2022.3 reads only PNG and JPEG into a texture (Texture2D.LoadImage).</summary>
        public static bool CanPreviewImage(MdixBlobKind kind) { return kind == MdixBlobKind.Png || kind == MdixBlobKind.Jpeg; }

        /// <summary>Unity's own decoders cover WAV and Ogg Vorbis for the editor's preview player.</summary>
        public static bool CanPlay(MdixBlobKind kind) { return kind == MdixBlobKind.Wav || kind == MdixBlobKind.Ogg; }

        /// <summary>The whole data, or false when the base64 is not valid.</summary>
        public static bool TryDecode(string base64, out byte[] bytes)
        {
            try
            {
                bytes = Convert.FromBase64String(base64 ?? string.Empty);
                return true;
            }
            catch (FormatException)
            {
                bytes = null;
                return false;
            }
        }

        /// <summary>
        /// A file name for "Save as": the last part of the value's path, made safe, plus the type's extension.
        /// <c>items[2].icon</c> gives <c>icon.png</c> for a PNG; nothing usable gives <c>blob</c>.
        /// </summary>
        public static string SuggestedFileName(string path, MdixBlobKind kind)
        {
            var name = path ?? string.Empty;

            while (name.EndsWith("]", StringComparison.Ordinal))      // drop trailing [n]
            {
                var open = name.LastIndexOf('[');
                if (open < 0) break;
                name = name.Substring(0, open);
            }

            var dot = name.LastIndexOf('.');
            if (dot >= 0) name = name.Substring(dot + 1);

            var sb = new StringBuilder(name.Length);
            foreach (var c in name)
                sb.Append((char.IsLetterOrDigit(c) && c < 128) || c == '-' || c == '_' ? c : '_');

            var safe = sb.ToString().Trim('_');
            if (safe.Length == 0) safe = "blob";
            return safe + Extension(kind);
        }

        /// <summary>The size an image is shown at: scaled down to fit the box, never up, keeping its shape.</summary>
        public static void FitSize(int width, int height, int maxWidth, int maxHeight, out int fitWidth, out int fitHeight)
        {
            if (width <= 0 || height <= 0)
            {
                fitWidth = maxWidth; fitHeight = maxHeight;
                return;
            }

            var scale = Math.Min(1.0, Math.Min((double)maxWidth / width, (double)maxHeight / height));
            fitWidth  = Math.Max(1, (int)Math.Round(width * scale));
            fitHeight = Math.Max(1, (int)Math.Round(height * scale));
        }

        // ── Previews ──────────────────────────────────────────────────────────

        /// <summary>
        /// A hex dump of the first bytes, 16 to a line: offset, hex in two groups of eight, and the printable
        /// characters. <paramref name="totalLength"/> is the whole blob's size, for the "more bytes" line.
        /// </summary>
        public static string HexDump(byte[] head, long totalLength)
        {
            if (head == null || head.Length == 0) return string.Empty;

            var shown = Math.Min(head.Length, HexDumpBytes);
            var sb    = new StringBuilder(shown * 4 + 64);

            for (int line = 0; line < shown; line += 16)
            {
                sb.Append(line.ToString("X8", Inv)).Append("  ");

                for (int i = 0; i < 16; i++)
                {
                    if (line + i < shown) sb.Append(head[line + i].ToString("X2", Inv));
                    else                  sb.Append("  ");
                    sb.Append(' ');
                    if (i == 7) sb.Append(' ');
                }

                sb.Append(" |");
                for (int i = 0; i < 16 && line + i < shown; i++)
                {
                    var b = head[line + i];
                    sb.Append(b >= 0x20 && b <= 0x7E ? (char)b : '.');
                }
                sb.Append('|').Append('\n');
            }

            if (totalLength > shown) sb.Append("… ").Append((totalLength - shown).ToString(Inv)).Append(" more bytes");
            return sb.ToString().TrimEnd('\n');
        }

        /// <summary>
        /// The start of a text blob as text: UTF-8, line endings as \n, other control characters shown as a dot, cut after
        /// <see cref="TextPreviewChars"/> characters. <paramref name="truncated"/> says when there is more than is shown.
        /// </summary>
        public static string TextPreview(byte[] head, long totalLength, out bool truncated)
        {
            truncated = false;
            if (head == null || head.Length == 0) return string.Empty;

            var used = Math.Min(head.Length, TextPreviewBytes);
            var text = Encoding.UTF8.GetString(head, 0, used).Replace("\r\n", "\n").Replace('\r', '\n');

            if (text.Length > TextPreviewChars)
            {
                text = text.Substring(0, TextPreviewChars);
                truncated = true;
            }
            if (totalLength > used) truncated = true;

            var sb = new StringBuilder(text.Length);
            foreach (var c in text)
                sb.Append(c < 0x20 && c != '\n' && c != '\t' ? '.' : c);
            return sb.ToString();
        }

        // ── Text for the Explorer ─────────────────────────────────────────────

        /// <summary>For example "4.2 KB". Sizes are in units of 1024.</summary>
        public static string FormatSize(long bytes)
        {
            if (bytes < 0)               return "?";
            if (bytes < 1024)            return bytes.ToString(Inv) + " B";
            if (bytes < 1024L * 1024)    return (bytes / 1024.0).ToString("0.0", Inv) + " KB";
            if (bytes < 1024L * 1024 * 1024) return (bytes / (1024.0 * 1024)).ToString("0.0", Inv) + " MB";
            return (bytes / (1024.0 * 1024 * 1024)).ToString("0.0", Inv) + " GB";
        }

        /// <summary>The Explorer's text for a blob value, for example "blob  4.2 KB  PNG".</summary>
        public static string Summary(string base64)
        {
            var size = DecodedLength(base64);
            if (size < 0)  return "blob  invalid base64";
            if (size == 0) return "blob  empty";

            var head = HeadBytes(base64, SniffBytes);
            return "blob  " + FormatSize(size) + "  " + ShortLabel(Sniff(head, head.Length));
        }

        /// <summary>
        /// A short value that changes when the blob's text changes, so an edit can be checked without keeping
        /// two copies of a large string: its length, then a 32-bit FNV-1a hash of its characters.
        /// </summary>
        public static string Fingerprint(string base64)
        {
            if (base64 == null) return "0:00000000";

            uint hash = 2166136261;
            foreach (var c in base64)
            {
                hash ^= c;
                hash = unchecked(hash * 16777619);
            }
            return base64.Length.ToString(Inv) + ":" + hash.ToString("x8", Inv);
        }

        // ── Small readers ─────────────────────────────────────────────────────

        private static bool StartsWith(byte[] b, int count, params int[] signature)
        {
            if (count < signature.Length) return false;
            for (int i = 0; i < signature.Length; i++)
                if (b[i] != signature[i]) return false;
            return true;
        }

        private static bool Ascii(byte[] b, int count, int offset, string text)
        {
            if (count < offset + text.Length) return false;
            for (int i = 0; i < text.Length; i++)
                if (b[offset + i] != (byte)text[i]) return false;
            return true;
        }
    }
}
