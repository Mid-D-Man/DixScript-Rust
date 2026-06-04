// com.midmanstudio.mdix.localization/Runtime/Core/MdixLocaleMetadata.cs
namespace MidManStudio.Mdix.Localization
{
    /// <summary>
    /// Immutable grammar and formatting metadata parsed from locale_* keys
    /// in a .mdix locale file. Read once on table construction and stored as
    /// a value type — zero allocation on every Get/GetPlural call.
    ///
    /// Keys consumed from the .mdix (all optional, defaults applied if absent):
    ///   locale_bcp47          string   "en-US"
    ///   locale_display_name   string   "English (US)"
    ///   locale_plural_rule    enum     PluralRule.ONE_OTHER
    ///   locale_script_dir     enum     ScriptDir.LTR
    ///   locale_gender_sys     enum     GenderSystem.NONE
    ///   fmt.decimal_sep       string   "."
    ///   fmt.thousands_sep     string   ","
    ///   fmt.date_pattern      string   "MM/DD/YYYY"
    /// </summary>
    public readonly struct MdixLocaleMetadata
    {
        /// <summary>BCP-47 language tag, e.g. "en-US" or "ru-RU".</summary>
        public string Bcp47 { get; }

        /// <summary>Display name in the locale's own script, e.g. "Русский".</summary>
        public string DisplayName { get; }

        /// <summary>
        /// Plural rule identifier read from the locale_plural_rule enum field value.
        /// One of: ONE_OTHER, ZERO_ONE_OTHER, SLAVIC, ARABIC, NONE.
        /// Passed to MdixPluralResolver.GetFormName() on every GetPlural call.
        /// </summary>
        public string PluralRule { get; }

        /// <summary>"LTR" or "RTL". Use IsRightToLeft for convenience.</summary>
        public string ScriptDir { get; }

        /// <summary>"NONE", "MASC_FEM", or "FULL".</summary>
        public string GenderSystem { get; }

        /// <summary>Decimal separator character, e.g. "." or ",".</summary>
        public string DecimalSep { get; }

        /// <summary>Thousands separator, e.g. "," (en_US) or " " (ru_RU).</summary>
        public string ThousandsSep { get; }

        /// <summary>Date format pattern, e.g. "MM/DD/YYYY" or "DD.MM.YYYY".</summary>
        public string DatePattern { get; }

        /// <summary>True when the locale is written right-to-left (Arabic, Hebrew, etc.).</summary>
        public bool IsRightToLeft => ScriptDir == "RTL";

        /// <summary>Sensible English-US defaults used when a locale omits optional keys.</summary>
        public static readonly MdixLocaleMetadata Default = new MdixLocaleMetadata(
            bcp47:        "en-US",
            displayName:  "English",
            pluralRule:   "ONE_OTHER",
            scriptDir:    "LTR",
            genderSystem: "NONE",
            decimalSep:   ".",
            thousandsSep: ",",
            datePattern:  "MM/DD/YYYY");

        public MdixLocaleMetadata(
            string bcp47,
            string displayName,
            string pluralRule,
            string scriptDir,
            string genderSystem,
            string decimalSep,
            string thousandsSep,
            string datePattern)
        {
            Bcp47        = bcp47        ?? "en-US";
            DisplayName  = displayName  ?? string.Empty;
            PluralRule   = pluralRule   ?? "ONE_OTHER";
            ScriptDir    = scriptDir    ?? "LTR";
            GenderSystem = genderSystem ?? "NONE";
            DecimalSep   = decimalSep   ?? ".";
            ThousandsSep = thousandsSep ?? ",";
            DatePattern  = datePattern  ?? "MM/DD/YYYY";
        }
    }
}
