// The parts of the window that the popup partial touches. The real ones live in the other partial files.
using System;
using System.Threading.Tasks;
using UnityEngine.UIElements;
namespace MidManStudio.Mdix.Unity.Editor.Lsp
{
    internal sealed class MdixLspClient
    {
        public bool IsRunning { get; }
        public Task<MdixJsonValue> RequestTestRegexAsync(string pattern, string testText, TimeSpan? timeout = null) => null;
    }
}
namespace MidManStudio.Mdix.Unity.Editor
{
    public sealed partial class MdixEditorWindow
    {
        private VisualElement rootVisualElement { get; } = new VisualElement();
        private void EndExplorerEdit() { }
        private string _sourceText = string.Empty;
        private void SetStatus(string message, bool error) { }
        private string ApplyExplorerEdit(string path, string text, string expectedOldLiteral, Action closeEditor) => null;
        private static void EnableCodeFieldScroller(TextField field) { }
        private Lsp.MdixLspClient _lspClient;
        private Task<bool> EnsureLspClientReadyAsync() => Task.FromResult(false);
    }
}
