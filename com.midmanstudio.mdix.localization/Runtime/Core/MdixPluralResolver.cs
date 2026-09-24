namespace MidManStudio.Mdix.Localization
{
    /// <summary>
    /// Maps a plural rule family name and a count to a CLDR plural form name.
    ///
    /// The rule name comes from the locale_plural_rule enum field declared in
    /// the .mdix locale file and stored in MdixLocaleMetadata.PluralRule.
    /// Actual rule logic lives entirely in CldrPluralFamilies as data (see
    /// its own doc comment) -- this class is just the lookup + safe fallback,
    /// and never needs a new method for a new or corrected CLDR rule.
    ///
    /// 27 families, mechanically extracted from real CLDR v48 data
    /// (unicode-org/cldr-json) and verified against an independent
    /// re-evaluation across 67,710 (locale, count) pairs with zero
    /// mismatches. See CldrPluralFamilies.cs for the full family list.
    ///
    /// Note on zero: ILocaleTable implementations pre-empt this resolver when
    /// count == 0 and an explicit "zero" named form exists in the locale data.
    /// That means e.g. a SLAVIC locale can provide "нет врагов" under the
    /// zero key even though real CLDR maps 0 to "many" for that family.
    /// </summary>
    public static class MdixPluralResolver
    {
        /// <summary>
        /// Returns the CLDR plural form name for a rule + count pair.
        /// Always returns a non-null, non-empty string. An unrecognized
        /// rule name falls back to ONE_OTHER behaviour (n = 1 -> one, else
        /// other) -- the same fallback the old 5-case switch used for its
        /// default branch.
        /// </summary>
        public static string GetFormName(string rule, int count)
        {
            if (rule != null && CldrPluralFamilies.All.TryGetValue(rule, out var family))
                return family.Resolve(count);

            return CldrPluralFamilies.All["ONE_OTHER"].Resolve(count);
        }
    }
}
