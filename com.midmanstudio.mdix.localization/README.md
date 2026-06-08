<!-- com.midmanstudio.mdix.localization/README.md -->

# com.midmanstudio.mdix.localization

Unity localization package built on the DixScript (.mdix) runtime. Locale files
are self-describing .mdix files that carry their own plural rules, script direction,
and formatting metadata as enum values — no hardcoded language lists in C#.

## Requirements

- Unity 2022.3 LTS or later
- `com.midmanstudio.mdix` (runtime and editor packages)
- TextMeshPro

## Package contentsRuntime/
Core/       ILocaleTable, MdixLocaleMetadata, MdixPluralResolver,
LocaleDataAsset (SO), BakedLocaleTable
Manager/    MdixLocalizationManager (singleton MonoBehaviour)
Components/ MdixLocalizedString (auto-updating text component)
Editor/
Import/     MdixLocaleImporter   — CSV / JSON → .mdix
Export/     MdixLocaleExporter   — .mdix → translator CSV / JSON
Validation/ MdixLocaleValidator  — locale vs reference diff report
UI/         MdixLocalizationEditorWindow — Window → MDIX → Localization Studio
Samples~/
localization_helpers.mdix  — shared @ENUMS + @QUICKFUNCS library
en_US.mdix, fr_FR.mdix, ru_RU.mdix## Locale file structure

Every locale is a .mdix file. Two-tier @DATA rule: all flat `SimpleProperty`
entries must come before any grouped entries (`TableProperty` or `GroupArray`).
Each `TablePath` may appear exactly once with `:` or `::` — never re-declared.@CONFIG( version -> "2.0.0" )

@IMPORTS( loc from "localization_helpers.mdix" )

@DATA(
// ── Flat: locale metadata (loc.* enums from imported helpers)
locale_display_name<string> = "English (US)"
locale_bcp47<string>        = "en-US"
locale_plural_rule<enum>    = loc.PluralRule.ONE_OTHER
locale_script_dir<enum>     = loc.ScriptDir.LTR
locale_gender_sys<enum>     = loc.GenderSystem.NONE

// ── Flat: plural keys — p2/p4 produce named .one/.other/.few/.many sub-keys
plural_enemies = loc.p2("1 enemy", "{0} enemies")

// ── Flat: annotated keys — key() bakes note, max_chars, valid, warning
ui_new_game = loc.key("New Game", "Main menu start button", 16)

// ── Grouped: plain string tables (each TablePath defined exactly once)
fmt:  decimal_sep = ".", thousands_sep = ",", date_pattern = "MM/DD/YYYY"
ui:   play = "Play", settings = "Settings", back = "Back"
)## Plural system

CLDR-based plural resolution. The active locale's `locale_plural_rule` drives
form selection. Four built-in rules:

| Rule            | Languages                  | Forms                          |
|-----------------|----------------------------|--------------------------------|
| ONE_OTHER       | English, German, Spanish … | one, other                     |
| ZERO_ONE_OTHER  | French (formal)            | zero, one, other               |
| SLAVIC          | Russian, Polish, Ukrainian | zero*, one, few, many          |
| ARABIC          | Arabic                     | zero, one, two, few, many, other|

*Slavic zero form: if `.zero` sub-key exists and `count == 0`, it is used
directly — bypassing the rule resolver.

**p2** (two-form) and **p4** (four-form) quickfuncs from `localization_helpers.mdix`
produce the named sub-keys at parse time:plural_enemies = loc.p2("1 enemy", "{0} enemies")
// → plural_enemies.one   = "1 enemy"
// → plural_enemies.other = "{0} enemies"

plural_enemies = loc.p4("Нет врагов", "{0} враг", "{0} врага", "{0} врагов")
// → plural_enemies.zero = "Нет врагов"
// → plural_enemies.one  = "{0} враг"
// → plural_enemies.few  = "{0} врага"
// → plural_enemies.many = "{0} врагов"## Runtime API

```csharp
// String lookup
MdixLocalizationManager.Get("ui.play")
MdixLocalizationManager.Get("hud_score.value", 1250)

// Plural lookup — form selected by active locale's PluralRule
MdixLocalizationManager.GetPlural("plural_enemies", 5)

// Locale switching
MdixLocalizationManager.SetLocale("fr_FR")   // exact
MdixLocalizationManager.SetLocale("fr")       // partial — resolves to "fr_FR"

// Metadata
MdixLocalizationManager.Metadata.DatePattern
MdixLocalizationManager.Metadata.IsRightToLeft
MdixLocalizationManager.Metadata.PluralRule

// Runtime QA
var issues = MdixLocalizationManager.GetValidationIssues();
// Returns keys where key() detected a character limit violation (live tables only).
```

## MdixLocalizedString component

Attach alongside TextMeshProUGUI, TextMeshPro, or UI Text.
Updates automatically on `OnLocaleChanged`.

```csharp
// Standard mode — formatted string
label.SetArguments(player.Score).Refresh();

// Plural mode — enable _isPluralMode in Inspector
label.SetCount(enemyCount).Refresh();

// Chaining
scoreLabel.SetArguments(score).Refresh();
livesLabel.SetCount(lives).Refresh();
```

## Dual runtime path

| Path   | How                                            | When to use          |
|--------|------------------------------------------------|----------------------|
| Live   | `LocaleEntry.Asset` (MdixAsset → FFI)         | Development, Editor  |
| Baked  | `LocaleEntry.BakedAsset` (LocaleDataAsset SO) | Shipped / WebGL      |

Baked path has zero FFI cost and zero file parsing at runtime. When `BakedAsset`
is assigned in the Inspector, it takes priority over `Asset`.
Bake via **Window → MDIX → Localization Studio → Bake** tab.

## Editor workflow

**Localization Studio** (Window → MDIX → Localization Studio):

- **Overview** — scan project for MdixAsset files.
- **Import** — bring in a translator CSV or JSON and produce a .mdix locale file.
  Supported CSV layouts:
  - `Key | Value` (2-column)
  - `Key | Note | Max | Value` (4-column translator format)
  - `Key | en_US | fr_FR | …` (multi-locale, one column per language)
- **Export** — write a .mdix as translator CSV or JSON.
- **Validate** — diff a locale against a reference, report missing keys, empty
  translations, and character-limit violations.
- **Bake** — populate a `LocaleDataAsset` ScriptableObject from a .mdix file
  for shipping.

## Localization pipelineExport reference (en_US.mdix → en_US_export.csv)
Send CSV to translators — they fill in the Value column
Import back (fr_FR_translated.csv → fr_FR.mdix)
Validate (fr_FR vs en_US — fix missing keys and over-limit strings)
Bake (fr_FR.mdix → fr_FR_Baked.asset)
Assign BakedAsset to LocaleEntry in Inspector## Adding a new locale

1. Duplicate an existing locale .mdix from `Samples~/`.
2. Update `locale_display_name`, `locale_bcp47`, `locale_plural_rule`,
   `locale_script_dir`, `locale_gender_sys` in the flat section.
3. Replace every string value with the translated text.
4. Add a `LocaleEntry` to `MdixLocalizationManager._locales` in the Inspector
   with the matching `LocaleCode`.
5. Validate against the reference locale before shipping.
