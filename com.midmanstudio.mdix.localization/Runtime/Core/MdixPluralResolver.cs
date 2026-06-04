// com.midmanstudio.mdix.localization/Runtime/Core/MdixPluralResolver.cs
using System;

namespace MidManStudio.Mdix.Localization
{
    /// <summary>
    /// Maps a plural rule name and a count to a CLDR plural form name.
    ///
    /// The rule name comes from the locale_plural_rule enum field declared in the
    /// .mdix locale file and stored in MdixLocaleMetadata.PluralRule. No language
    /// names are hardcoded here — adding a new language only requires writing a
    /// .mdix file with the correct PluralRule enum value. This class only changes
    /// if an entirely new rule type needs to be supported.
    ///
    /// Supported rules and CLDR form names produced:
    ///   ONE_OTHER       →  one | other
    ///   ZERO_ONE_OTHER  →  zero | one | other
    ///   SLAVIC          →  one | few | many          (Russian, Ukrainian, Serbian, …)
    ///   ARABIC          →  zero | one | two | few | many | other
    ///   NONE            →  other                     (always — no plural distinction)
    ///
    /// Note on zero: ILocaleTable implementations pre-empt this resolver when
    /// count == 0 and an explicit "zero" named form exists in the locale data.
    /// That means Slavic locales can provide "нет врагов" under the zero key
    /// even though CLDR technically maps 0 to "many" in the Slavic rule set.
    /// </summary>
    public static class MdixPluralResolver
    {
        /// <summary>
        /// Returns the CLDR plural form name for a rule + count pair.
        /// Always returns a non-null, non-empty string.
        /// Unknown rule strings fall back to ONE_OTHER behaviour.
        /// </summary>
        public static string GetFormName(string rule, int count)
        {
            return rule switch
            {
                "ONE_OTHER"      => OneOther(count),
                "ZERO_ONE_OTHER" => ZeroOneOther(count),
                "SLAVIC"         => Slavic(count),
                "ARABIC"         => Arabic(count),
                "NONE"           => "other",
                _                => OneOther(count),
            };
        }

        // ── Rule implementations ──────────────────────────────────────────────

        // English, German, Spanish, Dutch, Swedish, Norwegian, Finnish, …
        // CLDR cardinal rule 1
        //   n = 1  → one
        //   else   → other
        private static string OneOther(int count) =>
            count == 1 ? "one" : "other";

        // French (pt_BR treats 0-1 as singular), Turkish, …
        // CLDR cardinal rule 2 extended with explicit zero form
        //   n = 0  → zero
        //   n = 1  → one
        //   else   → other
        private static string ZeroOneOther(int count)
        {
            if (count == 0) return "zero";
            if (count == 1) return "one";
            return "other";
        }

        // Russian, Ukrainian, Belarusian, Serbian, Croatian, Bulgarian, …
        // CLDR cardinal rule 7
        //   n%100 in 11–19  → many   (teen override takes priority)
        //   n%10 == 1       → one
        //   n%10 in 2–4     → few
        //   else            → many
        private static string Slavic(int count)
        {
            int abs  = Math.Abs(count);
            int n100 = abs % 100;
            int n10  = abs % 10;

            if (n100 >= 11 && n100 <= 19) return "many";
            if (n10  == 1)                 return "one";
            if (n10  >= 2 && n10 <= 4)    return "few";
            return "many";
        }

        // Arabic — CLDR cardinal rule 12 (6 forms)
        //   n = 0              → zero
        //   n = 1              → one
        //   n = 2              → two
        //   n%100 in 3..10     → few
        //   n%100 in 11..99    → many
        //   else               → other
        private static string Arabic(int count)
        {
            int abs  = Math.Abs(count);
            int n100 = abs % 100;

            if (abs == 0)                  return "zero";
            if (abs == 1)                  return "one";
            if (abs == 2)                  return "two";
            if (n100 >= 3  && n100 <= 10)  return "few";
            if (n100 >= 11 && n100 <= 99)  return "many";
            return "other";
        }
    }
}
