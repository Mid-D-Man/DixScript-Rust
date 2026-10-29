using System;
using System.IO;
using System.Text;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>
    /// Temporary copies of a blob's data, for playing it or opening it in another app. They live in the system's
    /// temp folder, never in the project, so Unity never imports them. Without Unity types, so it can be tested on its own.
    /// </summary>
    internal static class MdixBlobTemp
    {
        /// <summary>The folder the copies go in. Settable for tests.</summary>
        internal static string Root = Path.Combine(Path.GetTempPath(), "MdixStudio");

        /// <summary>Writes the bytes to a new file named like <c>audio-1a2b3c4d.wav</c> and returns its full path.</summary>
        public static string Write(byte[] bytes, string stem, string extension)
        {
            Directory.CreateDirectory(Root);

            var safe = new StringBuilder();
            foreach (var c in stem ?? string.Empty)
                safe.Append(char.IsLetterOrDigit(c) && c < 128 ? c : '_');
            if (safe.Length == 0) safe.Append("blob");

            var name = safe + "-" + Guid.NewGuid().ToString("N").Substring(0, 8) + (extension ?? string.Empty);
            var path = Path.Combine(Root, name);
            File.WriteAllBytes(path, bytes ?? new byte[0]);
            return path;
        }

        /// <summary>Deletes a file made by <see cref="Write"/>; does nothing if it is gone or still in use.</summary>
        public static void Delete(string path)
        {
            try
            {
                if (!string.IsNullOrEmpty(path) && File.Exists(path)) File.Delete(path);
            }
            catch (IOException) { }
            catch (UnauthorizedAccessException) { }
        }

        /// <summary>Deletes copies older than <paramref name="age"/>. A file handed to another app cannot be deleted right away, so this tidies up later.</summary>
        public static void CleanOld(TimeSpan age)
        {
            try
            {
                if (!Directory.Exists(Root)) return;

                var cutoff = DateTime.UtcNow - age;
                foreach (var file in Directory.GetFiles(Root))
                {
                    if (File.GetLastWriteTimeUtc(file) < cutoff) Delete(file);
                }
            }
            catch (IOException) { }
            catch (UnauthorizedAccessException) { }
        }
    }
}
