using System;
using TMPro;
using UnityEngine;
using UnityEngine.UI;

namespace MidManStudio.Mdix.Localization
{
    /// <summary>
    /// Component that automatically updates a text element when the active
    /// locale changes. Attach alongside TextMeshProUGUI, TextMeshPro, or
    /// legacy UI Text.
    ///
    /// Standard mode (default):
    ///   Resolves _key via MdixLocalizationManager.Get().
    ///   Use _staticArguments or SetArguments() for {0}/{1} format slots.
    ///   Example: key = "gameplay.score", arg = player.Score
    ///              → "Score: 1250"
    ///
    /// Plural mode (_isPluralMode = true):
    ///   Resolves _key via MdixLocalizationManager.GetPlural(key, count).
    ///   CLDR form selection is driven by the active locale's PluralRule.
    ///   Set count via Inspector _pluralCount or runtime SetCount().
    ///   Example: key = "plural_enemies", count = 3
    ///              → "3 enemies" (en_US) / "3 врага" (ru_RU, few form)
    ///
    /// Chaining API (returns this for fluent one-liners):
    ///   label.SetArguments(score).Refresh();
    ///   label.SetCount(enemyCount).Refresh();
    /// </summary>
    [AddComponentMenu("MDIX/Localization/Mdix Localized String")]
    public sealed class MdixLocalizedString : MonoBehaviour
    {
        // ── Inspector ─────────────────────────────────────────────────────────

        [Tooltip("Dotted key path into the locale file, e.g. 'ui.play' or 'plural_enemies'.")]
        [SerializeField] private string _key = string.Empty;

        [Tooltip("Optional String.Format arguments for parameterised strings in standard mode. " +
                 "Ignored when Plural Mode is enabled.")]
        [SerializeField] private string[] _staticArguments = Array.Empty<string>();

        [Tooltip("When enabled, calls GetPlural(key, count) instead of Get(key). " +
                 "Use for any key backed by a p2/p4 plural helper or a plain :: array.")]
        [SerializeField] private bool _isPluralMode;

        [Tooltip("The plural count. Used as the Inspector / startup default. " +
                 "Override at runtime with SetCount(). Ignored in standard mode.")]
        [SerializeField] private int _pluralCount = 1;

        [Tooltip("When true, the key string itself is displayed if no translation is found. " +
                 "Set to false for optional UI elements that should stay blank when missing.")]
        [SerializeField] private bool _showKeyAsFallback = true;

        // ── Runtime state ─────────────────────────────────────────────────────

        private object[]? _runtimeArguments;

        private int  _runtimeCount;
        private bool _hasRuntimeCount;

        private TextMeshProUGUI? _tmpUgui;
        private TextMeshPro?     _tmp;
        private Text?            _legacyText;

        // ── Properties ────────────────────────────────────────────────────────

        /// <summary>
        /// The localization key. Assigning this property triggers an immediate Refresh.
        /// </summary>
        public string Key
        {
            get => _key;
            set { _key = value; Refresh(); }
        }

        /// <summary>
        /// Whether plural mode is active. Assigning this property triggers Refresh.
        /// </summary>
        public bool IsPluralMode
        {
            get => _isPluralMode;
            set { _isPluralMode = value; Refresh(); }
        }

        /// <summary>
        /// Runtime format arguments for standard mode {0}/{1} slots.
        /// Assigning via this property does NOT trigger Refresh — call it explicitly.
        /// Prefer SetArguments() for chaining.
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
        /// Set runtime String.Format arguments for standard mode and return this.
        /// Does NOT trigger Refresh — call it explicitly.
        /// Example: scoreLabel.SetArguments(player.Score).Refresh();
        /// </summary>
        public MdixLocalizedString SetArguments(params object[] args)
        {
            _runtimeArguments = args;
            return this;
        }

        /// <summary>
        /// Set the plural count for plural mode and return this.
        /// Does NOT trigger Refresh — call it explicitly.
        /// Example: enemyLabel.SetCount(spawnedCount).Refresh();
        /// </summary>
        public MdixLocalizedString SetCount(int count)
        {
            _runtimeCount    = count;
            _hasRuntimeCount = true;
            return this;
        }

        /// <summary>
        /// Clear the runtime count override so _pluralCount from the Inspector
        /// is used again. Does NOT trigger Refresh.
        /// </summary>
        public MdixLocalizedString ClearCount()
        {
            _hasRuntimeCount = false;
            return this;
        }

        /// <summary>
        /// Forces an immediate text update using the current key, arguments, and count.
        /// Called automatically on OnEnable and when the locale changes.
        /// </summary>
        public void Refresh()
        {
            if (string.IsNullOrEmpty(_key)) return;

            string text;

            if (_isPluralMode)
            {
                // Plural mode: prefer the runtime count; fall back to Inspector default.
                var count = _hasRuntimeCount ? _runtimeCount : _pluralCount;
                text = MdixLocalizationManager.GetPlural(_key, count);
            }
            else
            {
                // Standard mode: prefer runtime args, then static args, then bare get.
                var args = _runtimeArguments
                    ?? (_staticArguments.Length > 0
                        ? (object[])_staticArguments
                        : null);

                text = args != null && args.Length > 0
                    ? MdixLocalizationManager.Get(_key, args)
                    : MdixLocalizationManager.Get(_key);
            }

            // When key-as-fallback is disabled, leave the text unchanged if
            // the manager returned the raw key (no translation found).
            if (!_showKeyAsFallback &&
                string.Equals(text, _key, StringComparison.Ordinal))
                return;

            SetText(text);
        }

        // ── Private ───────────────────────────────────────────────────────────

        private void OnLocaleChanged(string _) => Refresh();

        private void SetText(string text)
        {
            if (_tmpUgui    != null) { _tmpUgui.text    = text; return; }
            if (_tmp        != null) { _tmp.text        = text; return; }
            if (_legacyText != null) { _legacyText.text = text; }
        }
    }
}
