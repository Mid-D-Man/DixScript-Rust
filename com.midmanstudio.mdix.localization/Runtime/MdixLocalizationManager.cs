// com.midmanstudio.mdix.localization/Runtime/MdixLocalizationManager.cs
using System;
using System.Collections.Generic;
using System.IO;
using System.Threading.Tasks;
using MidManStudio.Mdix.Core;
using MidManStudio.Mdix.Unity;
using UnityEngine;

namespace MidManStudio.Mdix.Localization
{
    /// <summary>
    /// Singleton MonoBehaviour that manages locale loading and switching.
    ///
    /// Setup:
    ///   1. Attach to a DontDestroyOnLoad GameObject.
    ///   2. Assign MdixAsset references for each locale in the Inspector.
    ///   3. Assign the defaultLocaleAsset (used as fallback).
    ///   4. Call SetLocale("fr_FR") to switch at runtime.
    ///
    /// Access strings:
    ///   MdixLocalizationManager.Get("ui.play")
    ///   MdixLocalizationManager.Get("gameplay.score", 1250)
    ///   MdixLocalizationManager.GetPlural("plural_coins", 5)
    /// </summary>
    public sealed class MdixLocalizationManager : MonoBehaviour
    {
        // ── Static access ─────────────────────────────────────────────────────

        public static MdixLocalizationManager? Instance { get; private set; }

        /// <summary>Fired whenever the active locale changes.</summary>
        public static event Action<string>? OnLocaleChanged;

        // ── Inspector fields ──────────────────────────────────────────────────

        [Header("Locale Assets")]
        [Tooltip("The locale used as fallback when a key is missing in the active locale.")]
        [SerializeField] private MdixAsset? _defaultLocaleAsset;

        [Tooltip("All available locale assets. Code is read from locale_display_name config.")]
        [SerializeField] private LocaleEntry[] _locales = Array.Empty<LocaleEntry>();

        [Header("Settings")]
        [Tooltip("Starting locale code, e.g. 'en_US'. Falls back to system language.")]
        [SerializeField] private string _initialLocaleCode = "en_US";

        [Tooltip("When enabled, locale files hot-reload on disk change (Editor/development only).")]
        [SerializeField] private bool _enableHotReload;

        // ── Runtime state ─────────────────────────────────────────────────────

        private MdixLocaleTable? _defaultTable;
        private MdixLocaleTable? _activeTable;
        private string           _activeLocaleCode = string.Empty;

        private readonly Dictionary<string, MdixAsset> _assetByCode =
            new Dictionary<string, MdixAsset>(StringComparer.OrdinalIgnoreCase);

        // ── Unity lifecycle ───────────────────────────────────────────────────

        private void Awake()
        {
            if (Instance != null && Instance != this)
            {
                Destroy(gameObject);
                return;
            }

            Instance = this;
            DontDestroyOnLoad(gameObject);

            // Index available locales by code.
            foreach (var entry in _locales)
            {
                if (entry.Asset != null && !string.IsNullOrEmpty(entry.LocaleCode))
                    _assetByCode[entry.LocaleCode] = entry.Asset;
            }

            // Load default locale first (used as fallback).
            LoadDefaultLocale();

            // Determine starting locale.
            var startCode = ResolveStartingLocale();
            SetLocale(startCode);
        }

        private void OnDestroy()
        {
            if (Instance == this)
            {
                _activeTable?.Dispose();
                _defaultTable?.Dispose();
                Instance = null;
            }
        }

        // ── Public API ────────────────────────────────────────────────────────

        /// <summary>Get a localized string by key path.</summary>
        public static string Get(string key) =>
            Instance?._activeTable?.Get(key) ?? key;

        /// <summary>Get a localized string with String.Format substitutions.</summary>
        public static string Get(string key, params object[] args) =>
            Instance?._activeTable?.Get(key, args) ?? key;

        /// <summary>Get a plural-aware localized string based on count.</summary>
        public static string GetPlural(string key, int count) =>
            Instance?._activeTable?.GetPlural(key, count) ?? key;

        /// <summary>Returns true if the given key exists in the active or fallback locale.</summary>
        public static bool HasKey(string key) =>
            Instance?._activeTable?.HasKey(key) ?? false;

        /// <summary>The currently active locale code (e.g. "en_US").</summary>
        public static string ActiveLocaleCode =>
            Instance?._activeLocaleCode ?? string.Empty;

        /// <summary>
        /// Switch to the locale identified by <paramref name="localeCode"/>.
        /// Returns true if the switch succeeded, false if the locale is unknown.
        /// </summary>
        public static bool SetLocale(string localeCode)
        {
            if (Instance == null) return false;
            return Instance.LoadLocale(localeCode);
        }

        /// <summary>All locale codes registered in the Inspector.</summary>
        public static IReadOnlyDictionary<string, MdixAsset> AvailableLocales =>
            Instance?._assetByCode ?? new Dictionary<string, MdixAsset>();

        // ── Private loading ───────────────────────────────────────────────────

        private void LoadDefaultLocale()
        {
            if (_defaultLocaleAsset == null)
            {
                Debug.LogWarning("[MdixLocalization] No default locale asset assigned.");
                return;
            }

            var dbResult = _defaultLocaleAsset.Load();
            if (dbResult.IsFailure)
            {
                Debug.LogError(
                    $"[MdixLocalization] Failed to load default locale: {dbResult.Error.Message}");
                return;
            }

            _defaultTable?.Dispose();
            var code = "default";
            _defaultTable = new MdixLocaleTable(code, dbResult.SuccessResult);
        }

        private bool LoadLocale(string localeCode)
        {
            if (string.Equals(localeCode, _activeLocaleCode, StringComparison.OrdinalIgnoreCase))
                return true;

            if (!_assetByCode.TryGetValue(localeCode, out var asset))
            {
                // Try matching without region (e.g. "fr" matches "fr_FR").
                foreach (var kv in _assetByCode)
                {
                    if (kv.Key.StartsWith(localeCode, StringComparison.OrdinalIgnoreCase))
                    {
                        asset      = kv.Value;
                        localeCode = kv.Key;
                        break;
                    }
                }

                if (asset == null)
                {
                    Debug.LogWarning(
                        $"[MdixLocalization] Locale '{localeCode}' not found. " +
                        "Check that the locale asset is assigned in the Inspector.");
                    return false;
                }
            }

            var dbResult = asset.Load();
            if (dbResult.IsFailure)
            {
                Debug.LogError(
                    $"[MdixLocalization] Failed to load locale '{localeCode}': " +
                    dbResult.Error.Message);
                return false;
            }

            var oldTable = _activeTable;
            _activeTable = new MdixLocaleTable(localeCode, dbResult.SuccessResult, _defaultTable);

            _activeLocaleCode = localeCode;

            if (_enableHotReload && Application.isEditor)
                dbResult.SuccessResult.EnableHotReload();

            OnLocaleChanged?.Invoke(_activeLocaleCode);

            // Dispose old table after firing the event (listeners may still read from it).
            oldTable?.Dispose();

            Debug.Log($"[MdixLocalization] Switched to locale: {localeCode}");
            return true;
        }

        private string ResolveStartingLocale()
        {
            if (!string.IsNullOrEmpty(_initialLocaleCode) &&
                _assetByCode.ContainsKey(_initialLocaleCode))
                return _initialLocaleCode;

            // Try to match system language.
            var lang = Application.systemLanguage;
            var code = SystemLanguageToCode(lang);

            if (!string.IsNullOrEmpty(code) && _assetByCode.ContainsKey(code))
                return code;

            // Fall back to first registered locale.
            foreach (var kv in _assetByCode)
                return kv.Key;

            return _initialLocaleCode;
        }

        private static string SystemLanguageToCode(SystemLanguage lang) => lang switch
        {
            SystemLanguage.English  => "en_US",
            SystemLanguage.French   => "fr_FR",
            SystemLanguage.German   => "de_DE",
            SystemLanguage.Spanish  => "es_ES",
            SystemLanguage.Japanese => "ja_JP",
            SystemLanguage.Korean   => "ko_KR",
            SystemLanguage.Chinese  => "zh_CN",
            SystemLanguage.Russian  => "ru_RU",
            SystemLanguage.Arabic   => "ar_SA",
            SystemLanguage.Italian  => "it_IT",
            SystemLanguage.Polish   => "pl_PL",
            _                       => string.Empty,
        };

        // ── Inspector data ────────────────────────────────────────────────────

        [Serializable]
        public sealed class LocaleEntry
        {
            [Tooltip("IETF language tag, e.g. en_US, fr_FR, ja_JP")]
            public string LocaleCode = string.Empty;
            public MdixAsset? Asset;
        }
    }
}
