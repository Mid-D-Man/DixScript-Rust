// com.midmanstudio.mdix.localization/Runtime/MdixLocalizedString.cs
using System;
using TMPro;
using UnityEngine;
using UnityEngine.UI;

namespace MidManStudio.Mdix.Localization
{
    /// <summary>
    /// Component that automatically updates a text element when the locale changes.
    ///
    /// Attach alongside a <see cref="TextMeshProUGUI"/>, <see cref="TextMeshPro"/>,
    /// or legacy <see cref="Text"/> component. The key is resolved via
    /// <see cref="MdixLocalizationManager.Get"/> each time the locale changes.
    ///
    /// For dynamic values (scores, counts), set <see cref="Arguments"/> at
    /// runtime and call <see cref="Refresh"/> to update the display.
    /// </summary>
    public sealed class MdixLocalizedString : MonoBehaviour
    {
        // ── Inspector ─────────────────────────────────────────────────────────

        [Tooltip("Dotted key path into the locale file, e.g. 'gameplay.score' or 'ui.play'.")]
        [SerializeField] private string _key = string.Empty;

        [Tooltip("Optional String.Format arguments for parameterised strings like 'Score: {0}'.")]
        [SerializeField] private string[] _staticArguments = Array.Empty<string>();

        [Tooltip("When true, falls back to the key itself if the translation is missing.")]
        [SerializeField] private bool _showKeyAsFallback = true;

        // ── Runtime state ─────────────────────────────────────────────────────

        // Overrides for dynamic values (set via SetArguments / Refresh).
        private object[]? _runtimeArguments;

        // Cached component references.
        private TextMeshProUGUI? _tmpUgui;
        private TextMeshPro?     _tmp;
        private Text?            _legacyText;

        // ── Properties ────────────────────────────────────────────────────────

        /// <summary>The localization key used to look up the string.</summary>
        public string Key
        {
            get => _key;
            set { _key = value; Refresh(); }
        }

        /// <summary>
        /// Override runtime arguments for parameterised strings.
        /// Call <see cref="Refresh"/> after setting to update the displayed text.
        /// </summary>
        public object[]? Arguments
        {
            get => _runtimeArguments;
            set => _runtimeArguments = value;
        }

        // ── Unity lifecycle ───────────────────────────────────────────────────

        private void Awake()
        {
            _tmpUgui    = GetComponent<TextMeshProUGUI>();
            _tmp        = GetComponent<TextMeshPro>();
            _legacyText = GetComponent<Text>();

            if (_tmpUgui == null && _tmp == null && _legacyText == null)
            {
                Debug.LogWarning(
                    $"[MdixLocalizedString] No Text component found on '{name}'. " +
                    "Add TextMeshProUGUI, TextMeshPro, or UI Text.",
                    this);
            }
        }

        private void OnEnable()
        {
            MdixLocalizationManager.OnLocaleChanged += OnLocaleChanged;
            Refresh();
        }

        private void OnDisable()
        {
            MdixLocalizationManager.OnLocaleChanged -= OnLocaleChanged;
        }

        // ── Public API ────────────────────────────────────────────────────────

        /// <summary>
        /// Set dynamic arguments and immediately refresh the displayed text.
        /// Example: <c>label.SetArguments(player.Score).Refresh();</c>
        /// </summary>
        public MdixLocalizedString SetArguments(params object[] args)
        {
            _runtimeArguments = args;
            return this;
        }

        /// <summary>Forces an immediate text update using the current key and arguments.</summary>
        public void Refresh()
        {
            if (string.IsNullOrEmpty(_key)) return;

            var args = _runtimeArguments
                ?? (_staticArguments.Length > 0 ? (object[])_staticArguments : null);

            string text;
            if (args != null && args.Length > 0)
                text = MdixLocalizationManager.Get(_key, args);
            else
                text = MdixLocalizationManager.Get(_key);

            if (!_showKeyAsFallback && string.Equals(text, _key, StringComparison.Ordinal))
                return;

            SetText(text);
        }

        // ── Private helpers ───────────────────────────────────────────────────

        private void OnLocaleChanged(string _) => Refresh();

        private void SetText(string text)
        {
            if (_tmpUgui    != null) { _tmpUgui.text    = text; return; }
            if (_tmp        != null) { _tmp.text        = text; return; }
            if (_legacyText != null) { _legacyText.text = text; }
        }
    }
}
