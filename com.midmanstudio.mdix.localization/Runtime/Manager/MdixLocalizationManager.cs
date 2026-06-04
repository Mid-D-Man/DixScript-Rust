// com.midmanstudio.mdix.localization/Runtime/Manager/MdixLocalizationManager.cs
using System;
using System.Collections.Generic;
using MidManStudio.Mdix.Core;
using MidManStudio.Mdix.Unity;
using UnityEngine;

namespace MidManStudio.Mdix.Localization
{
    /// <summary>
    /// Singleton MonoBehaviour managing locale loading, switching, and lookup.
    ///
    /// Setup:
    ///   1. Attach to a DontDestroyOnLoad GameObject.
    ///   2. Assign the default locale (MdixAsset or LocaleDataAsset) in the Inspector.
    ///   3. Populate the Locales list with a LocaleEntry per language.
    ///      Each entry needs a LocaleCode and either Asset (development) or
    ///      BakedAsset (shipped builds, WebGL-safe) — or both.
    ///   4. Call SetLocale("fr_FR") to switch at runtime.
    ///
    /// Access strings:
    ///   MdixLocalizationManager.Get("ui.play")
    ///   MdixLocalizationManager.Get("gameplay.score", 1250)
    ///   MdixLocalizationManager.GetPlural("plural_enemies", 5)
    ///   MdixLocalizationManager.Metadata.DatePattern
    /// </summary>
    public sealed class MdixLocalizationManager : MonoBehaviour
    {
        // ── Static access ─────────────────────────────────────────────────────

        public static MdixLocalizationManager? Instance { get; private set; }

        /// <summary>Fired after the active locale changes. Arg is the new locale code.</summary>
        public static event Action<string>? OnLocaleChanged;

        // ── Inspector fields ──────────────────────────────────────────────────

        [Header("Default / Fallback Locale")]
        [Tooltip("Source .mdix asset for the fallback locale (development). " +
                 "Used when BakedAsset is not assigned.")]
        [SerializeField] private MdixAsset? _defaultLocaleAsset;

        [Tooltip("Baked ScriptableObject for the fallback locale (shipped builds). " +
                 "When assigned, takes priority over the .mdix asset — zero FFI cost.")]
        [SerializeField] private LocaleDataAsset? _defaultBakedAsset;

        [Header("Available Locales")]
        [Tooltip("All locale entries. Each needs a LocaleCode and at least one of " +
                 "Asset (live FFI) or BakedAsset (baked, WebGL-safe).")]
        [SerializeField] private LocaleEntry[] _locales = Array.Empty<LocaleEntry>();

        [Header("Settings")]
        [Tooltip("Locale code to load on startup, e.g. 'en_US'. " +
                 "Falls back to system language, then first registered locale.")]
        [SerializeField] private string _initialLocaleCode = "en_US";

        [Tooltip("Hot-reload .mdix files on disk change. Editor / development only.")]
        [SerializeField] private bool _enableHotReload;

        // ── Runtime state ─────────────────────────────────────────────────────

        private ILocaleTable? _defaultTable;
        private ILocaleTable? _activeTable;
        private string        _activeLocaleCode = string.Empty;

        // _assetByCode: retained for AvailableLocales backward compat
        //               (only contains entries where Asset != null).
        // _entryByCode: full index including baked-only entries.
        private readonly Dictionary<string, MdixAsset>   _assetByCode =
            new Dictionary<string, MdixAsset>(StringComparer.OrdinalIgnoreCase);
        private readonly Dictionary<string, LocaleEntry> _entryByCode =
            new Dictionary<string, LocaleEntry>(StringComparer.OrdinalIgnoreCase);

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

            foreach (var entry in _locales)
            {
                if (string.IsNullOrEmpty(entry.LocaleCode)) continue;

                _entryByCode[entry.LocaleCode] = entry;

                if (entry.Asset != null)
                    _assetByCode[entry.LocaleCode] = entry.Asset;
            }

            LoadDefaultLocale();
            SetLocale(ResolveStartingLocale());
        }

        private void OnDestroy()
        {
            if (Instance != this) return;

            _activeTable?.Dispose();
            _defaultTable?.Dispose();
            Instance = null;
        }

        // ── Public static API ─────────────────────────────────────────────────

        /// <summary>Get a localized string by key path.</summary>
        public static string Get(string key) =>
            Instance?._activeTable?.Get(key) ?? key;

        /// <summary>Get a localized string with String.Format substitutions.</summary>
        public static string Get(string key, params object[] args) =>
            Instance?._activeTable?.Get(key, args) ?? key;

        /// <summary>Get a plural-aware localized string based on count.</summary>
        public static string GetPlural(string key, int count) =>
            Instance?._activeTable?.GetPlural(key, count) ?? key;

        /// <summary>Returns true if the key exists in the active or fallback locale.</summary>
        public static bool HasKey(string key) =>
            Instance?._activeTable?.HasKey(key) ?? false;

        /// <summary>The currently active locale code, e.g. "en_US".</summary>
        public static string ActiveLocaleCode =>
            Instance?._activeLocaleCode ?? string.Empty;

        /// <summary>
        /// Grammar and formatting metadata for the active locale.
        /// Returns MdixLocaleMetadata.Default when no locale is loaded.
        /// Use for date formatting, decimal separators, RTL layout decisions, etc.
        /// </summary>
        public static MdixLocaleMetadata Metadata =>
            Instance?._activeTable?.Metadata ?? MdixLocaleMetadata.Default;

        /// <summary>
        /// Returns keys where the key() @QUICKFUNCS helper detected a character
        /// limit violation in the active locale. Useful for runtime QA checks.
        /// Returns empty when using a baked table — fix issues before baking.
        /// </summary>
        public static IReadOnlyList<MdixLocaleValidationIssue> GetValidationIssues() =>
            Instance?._activeTable is MdixLocaleTable live
                ? live.GetValidationIssues()
                : Array.Empty<MdixLocaleValidationIssue>();

        /// <summary>
        /// Switch to the locale identified by localeCode.
        /// Accepts partial codes: "fr" resolves to "fr_FR" if registered.
        /// Returns true on success, false if the locale is unknown.
        /// </summary>
        public static bool SetLocale(string localeCode)
        {
            if (Instance == null) return false;
            return Instance.LoadLocale(localeCode);
        }

        /// <summary>
        /// Locale codes that have a non-null MdixAsset assigned.
        /// For all registered codes (including baked-only) use AvailableLocaleCodes.
        /// Retained for backward compatibility.
        /// </summary>
        public static IReadOnlyDictionary<string, MdixAsset> AvailableLocales =>
            Instance?._assetByCode ??
            (IReadOnlyDictionary<string, MdixAsset>)new Dictionary<string, MdixAsset>();

        /// <summary>
        /// All registered locale codes, including entries with only a BakedAsset.
        /// </summary>
        public static IEnumerable<string> AvailableLocaleCodes =>
            Instance?._entryByCode.Keys ??
            (IEnumerable<string>)Array.Empty<string>();

        // ── Private loading ───────────────────────────────────────────────────

        private void LoadDefaultLocale()
        {
            // Baked path: WebGL-safe, zero FFI.
            if (_defaultBakedAsset != null)
            {
                _defaultTable?.Dispose();
                _defaultTable = new BakedLocaleTable(_defaultBakedAsset);
                return;
            }

            // Live FFI path.
            if (_defaultLocaleAsset == null)
            {
                Debug.LogWarning(
                    "[MdixLocalization] No default locale asset assigned. " +
                    "Assign _defaultLocaleAsset or _defaultBakedAsset in the Inspector.");
                return;
            }

            var dbResult = _defaultLocaleAsset.Load();
            if (dbResult.IsFailure)
            {
                Debug.LogError(
                    $"[MdixLocalization] Failed to load default locale: " +
                    dbResult.Error.Message);
                return;
            }

            _defaultTable?.Dispose();
            _defaultTable = new MdixLocaleTable("default", dbResult.SuccessResult);
        }

        private bool LoadLocale(string localeCode)
        {
            if (string.Equals(localeCode, _activeLocaleCode,
                    StringComparison.OrdinalIgnoreCase))
                return true;

            if (!TryFindEntry(localeCode, out var entry, out var resolvedCode))
            {
                Debug.LogWarning(
                    $"[MdixLocalization] Locale '{localeCode}' not found. " +
                    "Check the LocaleCode spelling and Inspector assignment.");
                return false;
            }

            ILocaleTable newTable;

            // ── Baked path ────────────────────────────────────────────────────
            if (entry.BakedAsset != null)
            {
                newTable = new BakedLocaleTable(entry.BakedAsset, _defaultTable);
            }
            // ── Live FFI path ─────────────────────────────────────────────────
            else
            {
                if (entry.Asset == null)
                {
                    Debug.LogWarning(
                        $"[MdixLocalization] Locale '{resolvedCode}' has neither a " +
                        "BakedAsset nor a MdixAsset assigned. " +
                        "Assign at least one in the Inspector.");
                    return false;
                }

                var dbResult = entry.Asset.Load();
                if (dbResult.IsFailure)
                {
                    Debug.LogError(
                        $"[MdixLocalization] Failed to load locale '{resolvedCode}': " +
                        dbResult.Error.Message);
                    return false;
                }

                newTable = new MdixLocaleTable(
                    resolvedCode, dbResult.SuccessResult, _defaultTable);

                if (_enableHotReload && Application.isEditor)
                    dbResult.SuccessResult.EnableHotReload();
            }

            var oldTable = _activeTable;
            _activeTable      = newTable;
            _activeLocaleCode = resolvedCode;

            // Fire event before disposing so listeners can still read from the manager.
            OnLocaleChanged?.Invoke(_activeLocaleCode);

            // Dispose after event — listeners may finish reading from the old table.
            oldTable?.Dispose();

            Debug.Log($"[MdixLocalization] Switched to locale: {resolvedCode} " +
                      $"({(entry.BakedAsset != null ? "baked" : "live")})");
            return true;
        }

        private bool TryFindEntry(
            string localeCode,
            out LocaleEntry entry,
            out string resolvedCode)
        {
            // Exact match.
            if (_entryByCode.TryGetValue(localeCode, out entry))
            {
                resolvedCode = localeCode;
                return true;
            }

            // Partial match: "fr" resolves to "fr_FR" when that code is registered.
            foreach (var kv in _entryByCode)
            {
                if (kv.Key.StartsWith(localeCode, StringComparison.OrdinalIgnoreCase))
                {
                    entry        = kv.Value;
                    resolvedCode = kv.Key;
                    return true;
                }
            }

            entry        = default!;
            resolvedCode = localeCode;
            return false;
        }

        private string ResolveStartingLocale()
        {
            if (!string.IsNullOrEmpty(_initialLocaleCode) &&
                _entryByCode.ContainsKey(_initialLocaleCode))
                return _initialLocaleCode;

            var code = SystemLanguageToCode(Application.systemLanguage);
            if (!string.IsNullOrEmpty(code) && _entryByCode.ContainsKey(code))
                return code;

            // Fall back to whichever locale was registered first.
            foreach (var k in _entryByCode.Keys) return k;

            return _initialLocaleCode;
        }

        private static string SystemLanguageToCode(SystemLanguage lang) => lang switch
        {
            SystemLanguage.English   => "en_US",
            SystemLanguage.French    => "fr_FR",
            SystemLanguage.German    => "de_DE",
            SystemLanguage.Spanish   => "es_ES",
            SystemLanguage.Japanese  => "ja_JP",
            SystemLanguage.Korean    => "ko_KR",
            SystemLanguage.Chinese   => "zh_CN",
            SystemLanguage.Russian   => "ru_RU",
            SystemLanguage.Arabic    => "ar_SA",
            SystemLanguage.Italian   => "it_IT",
            SystemLanguage.Polish    => "pl_PL",
            SystemLanguage.Ukrainian => "uk_UA",
            _                        => string.Empty,
        };

        // ── Inspector data ────────────────────────────────────────────────────

        [Serializable]
        public sealed class LocaleEntry
        {
            [Tooltip("IETF language tag, e.g. en_US, fr_FR, ja_JP, ru_RU.")]
            public string LocaleCode = string.Empty;

            [Tooltip("Source .mdix asset. Used at development time and on platforms " +
                     "with the native plugin when BakedAsset is not assigned.")]
            public MdixAsset? Asset;

            [Tooltip("Baked ScriptableObject (LocaleDataAsset). When assigned, used " +
                     "instead of Asset at runtime — zero FFI, zero parsing, WebGL-safe. " +
                     "Create via Window → MDIX → Localization Studio → Bake.")]
            public LocaleDataAsset? BakedAsset;
        }
    }
}
