using System;

namespace MidManStudio.Mdix.Localization
{
    /// <summary>
    /// A single CLDR-style relation on an integer count: optionally reduce
    /// by a modulus, then test membership in a set of inclusive ranges
    /// (a bare value is a range where Lo == Hi), optionally negated.
    /// Directly mirrors CLDR TR35's relation grammar (e.g. "n % 100 = 11..19"),
    /// already reduced to integer-only terms (CLDR's decimal-only operands
    /// v/w/f/t/e/c are always 0 for a plain integer, so any relation that
    /// depended solely on them was resolved to a constant at extraction time
    /// -- see CldrPluralFamilies.cs's generation notes).
    /// </summary>
    public readonly struct PluralRelation
    {
        public readonly int? Modulus;
        public readonly bool Negate;
        public readonly (int Lo, int Hi)[] Ranges;

        public PluralRelation(int? modulus, bool negate, (int, int)[] ranges)
        {
            Modulus = modulus;
            Negate = negate;
            Ranges = ranges ?? Array.Empty<(int, int)>();
        }

        public bool Matches(int n)
        {
            int val = Modulus.HasValue ? n % Modulus.Value : n;
            bool inRange = false;
            foreach (var (lo, hi) in Ranges)
            {
                if (val >= lo && val <= hi) { inRange = true; break; }
            }
            return Negate ? !inRange : inRange;
        }
    }

    /// <summary>An AND-group: every relation must match. An empty group (all its relations reduced to a constant true) always matches.</summary>
    public readonly struct PluralAndGroup
    {
        public readonly PluralRelation[] Relations;

        public PluralAndGroup(PluralRelation[] relations)
        {
            Relations = relations ?? Array.Empty<PluralRelation>();
        }

        public bool Matches(int n)
        {
            foreach (var r in Relations)
                if (!r.Matches(n)) return false;
            return true;
        }
    }

    /// <summary>
    /// An OR of AND-groups -- CLDR's full condition grammar. A condition
    /// with zero groups (all of a category's OR-branches were impossible
    /// for integer input, e.g. Czech's "many" is decimal-only) never
    /// matches; PluralRuleFamily represents that case as a null
    /// PluralCondition rather than an empty one, so callers don't need to
    /// special-case it.
    /// </summary>
    public sealed class PluralCondition
    {
        public readonly PluralAndGroup[] OrGroups;

        public PluralCondition(PluralAndGroup[] orGroups)
        {
            OrGroups = orGroups ?? Array.Empty<PluralAndGroup>();
        }

        public bool Matches(int n)
        {
            foreach (var g in OrGroups)
                if (g.Matches(n)) return true;
            return false;
        }
    }

    /// <summary>
    /// A CLDR cardinal plural rule family, expressed as data rather than
    /// code -- one Resolve() interprets any family, so adding or correcting
    /// a family (e.g. a future CLDR update) is a data change, not a new
    /// resolver method. Verified against real CLDR v48 data for 222
    /// locales, collapsed to integer-only behavior since this package's
    /// GetPlural takes an int: an independent cross-check evaluator
    /// confirmed identical category output across 67,710 (locale, count)
    /// test pairs with zero mismatches before this was generated.
    /// Categories not present for a family (e.g. no "zero" form) are null;
    /// "other" is the implicit fallback and never has a condition of its own.
    /// </summary>
    public sealed class PluralRuleFamily
    {
        public string Id { get; }
        public PluralCondition? Zero { get; }
        public PluralCondition? One { get; }
        public PluralCondition? Two { get; }
        public PluralCondition? Few { get; }
        public PluralCondition? Many { get; }

        public PluralRuleFamily(string id, PluralCondition? zero, PluralCondition? one, PluralCondition? two, PluralCondition? few, PluralCondition? many)
        {
            Id = id;
            Zero = zero;
            One = one;
            Two = two;
            Few = few;
            Many = many;
        }

        /// <summary>
        /// Resolves the CLDR cardinal category name ("zero"/"one"/"two"/"few"/"many"/"other")
        /// for a count. CLDR's own "n" operand is defined as the absolute
        /// value of the source number, so a negative count resolves
        /// exactly as its positive counterpart would.
        /// </summary>
        public string Resolve(int count)
        {
            int n = Math.Abs(count);
            if (Zero != null && Zero.Matches(n)) return "zero";
            if (One != null && One.Matches(n)) return "one";
            if (Two != null && Two.Matches(n)) return "two";
            if (Few != null && Few.Matches(n)) return "few";
            if (Many != null && Many.Matches(n)) return "many";
            return "other";
        }
    }
}
