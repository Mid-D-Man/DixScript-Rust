using System.IO;
using System.Text;
using UnityEditor;

namespace MidManStudio.Mdix.Unity.Editor
{
    /// <summary>
    /// Assets, Create, MDIX, Explorer Tour Samples: writes the sample files (see <see cref="MdixSampleData"/>) into a new
    /// folder of the project, next to whatever is selected in the Project window. Nothing is ever overwritten: when the
    /// folder already exists, a numbered one is made.
    /// </summary>
    internal static class MdixSampleMenu
    {
        [MenuItem("Assets/Create/MDIX/Explorer Tour Samples")]
        private static void Create()
        {
            var folder = AssetDatabase.GenerateUniqueAssetPath(SelectedFolder() + "/" + MdixSampleData.FolderName);
            Directory.CreateDirectory(folder);

            Write(folder, MdixSampleData.TourFileName,    MdixSampleData.Tour);
            Write(folder, MdixSampleData.NumbersFileName, MdixSampleData.Numbers);
            Write(folder, MdixSampleData.ReadmeFileName,  MdixSampleData.Readme);

            AssetDatabase.Refresh();

            var first = AssetDatabase.LoadMainAssetAtPath(folder + "/" + MdixSampleData.TourFileName);
            if (first == null) return;

            EditorUtility.FocusProjectWindow();
            Selection.activeObject = first;
            EditorGUIUtility.PingObject(first);
        }

        // Unix line endings and no byte-order mark, whatever the checkout used, so a raw string in a sample does not
        // pick up a stray carriage return.
        private static void Write(string folder, string name, string content)
        {
            File.WriteAllText(Path.Combine(folder, name), content.Replace("\r\n", "\n"), new UTF8Encoding(false));
        }

        // The folder that is selected, or the one holding the selected file; the project's Assets folder when nothing is
        // selected or the selection is inside a package, which cannot be written to.
        private static string SelectedFolder()
        {
            var path = AssetDatabase.GetAssetPath(Selection.activeObject);

            if (string.IsNullOrEmpty(path)) return "Assets";

            var folder = Directory.Exists(path) ? path : (Path.GetDirectoryName(path) ?? "Assets");
            folder = folder.Replace('\\', '/');

            return folder.StartsWith("Assets", System.StringComparison.Ordinal) ? folder : "Assets";
        }
    }
}
