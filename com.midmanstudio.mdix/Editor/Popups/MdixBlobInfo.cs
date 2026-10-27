using System;
using System.Globalization;

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
