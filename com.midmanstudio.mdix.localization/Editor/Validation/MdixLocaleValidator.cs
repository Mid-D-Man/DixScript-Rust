using System;
using System.Collections.Generic;
using System.Linq;
using MidManStudio.Mdix.Core;

namespace MidManStudio.Mdix.Localization.Editor
{
    // ── Report types ──────────────────────────────────────────────────────────

    public enum MdixLocaleIssueKind
    {
        /// <summary>Key exists in the reference locale but is absent in this locale.</summary>
        MissingKey,
        /// <summary>Translation exists but is an empty or whitespace-only string.</summary>
        EmptyTranslation,
        /// <summary>
        /// The key() @QUICKFUNCS helper detected a character limit violation
        /// (valid = false baked into the locale data at parse time).
        /// </summary>
        OverLimit,
    }

    public readonly struct MdixLocaleIssue
    {
        public MdixLocaleIssueKind Kind   { get; }
        public string              Key    { get; }
        public string              Detail { get; }

        public MdixLocaleIssue(MdixLocaleIssueKind kind, string key, string detail)
        {
            Kind = kind; Key = key; Detail = detail;
        }

        public override string ToString() => $"[{Kind}] {Key}: {Detail}";
    }

    public sealed class MdixLocaleReport
    {
        public bool                         IsValid       => Issues.Count == 0;
        public IReadOnlyList<MdixLocaleIssue> Issues      { get; }
        public int MissingCount   => Issues.Count(i => i.Kind == MdixLocaleIssueKind.MissingKey);
        public int EmptyCount     => Issues.Count(i => i.Kind == MdixLocaleIssueKind.EmptyTranslation);
        public int OverLimitCount => Issues.Count(i => i.Kind == MdixLocaleIssueKind.OverLimit);

        public MdixLocaleReport(IReadOnlyList<MdixLocaleIssue> issues)
        {
            Issues = issues ?? (IReadOnlyList<MdixLocaleIssue>)Array.Empty<MdixLocaleIssue>();
        }

        public override string ToString() =>
            IsValid
                ? "Validation passed — no issues."
                : $"Validation failed: {MissingCount} missing, {EmptyCount} empty, " +
                  $"{OverLimitCount} over limit.";
    }

    // ── Validator ─────────────────────────────────────────────────────────────

    /// <summary>
    /// Compares a locale database against a reference locale and produces a
    /// MdixLocaleReport. Useful before baking or shipping a translation.
    ///
    /// Checks performed:
    ///   1. Missing keys: every string/enum leaf in the reference that is
    ///      absent from the locale under validation.
    ///   2. Empty translations: keys that exist but contain only whitespace.
    ///   3. Over-limit: keys where the key() quickfunc baked valid = false
    ///      (value exceeded its declared max_chars at parse time).
    /// </summary>
    public static class MdixLocaleValidator
    {
        private static readonly HashSet<string> _metaSuffixes =
            new HashSet<string>(StringComparer.Ordinal)
            { ".valid", ".warning", ".note", ".max_chars" };

        /// <summary>
        /// Validate <paramref name="locale"/> against <paramref name="reference"/>.
        /// Neither database is disposed by this method.
        /// </summary>
        public static MdixLocaleReport Validate(
            MdixDatabase locale,
            MdixDatabase reference)
        {
            if (locale    == null) throw new ArgumentNullException(nameof(locale));
            if (reference == null) throw new ArgumentNullException(nameof(reference));

            var issues  = new List<MdixLocaleIssue>();
            var visited = new HashSet<string>(StringComparer.Ordinal);

            // ── 1. Missing and empty keys ─────────────────────────────────────
            CheckMissing(locale, reference, string.Empty, issues, visited, depth: 0);

            // ── 2. Over-limit keys from key() quickfunc ───────────────────────
            var locKeys = locale.GetKeys().UnwrapOr(Array.Empty<string>());
            foreach (var k in locKeys)
            {
                if (!k.EndsWith(".valid", StringComparison.Ordinal)) continue;

                var validResult = locale.GetBool(k);
                if (!validResult.IsSuccess || validResult.SuccessResult) continue;

                var baseKey = k.Substring(0, k.Length - ".valid".Length);
                var warning = locale.GetString($"{baseKey}.warning")
                                    .UnwrapOr("Exceeds declared max_chars limit.");

                issues.Add(new MdixLocaleIssue(MdixLocaleIssueKind.OverLimit, baseKey, warning));
            }

            return new MdixLocaleReport(issues);
        }

        // Recursively walks the reference database and checks each leaf against
        // the locale under validation.
        private static void CheckMissing(
            MdixDatabase       locale,
            MdixDatabase       reference,
            string             prefix,
            List<MdixLocaleIssue> issues,
            HashSet<string>    visited,
            int                depth)
        {
            if (depth > 8) return;

            var children = reference.GetKeys(prefix).UnwrapOr(Array.Empty<string>());

            foreach (var child in children)
            {
                var fullKey = string.IsNullOrEmpty(prefix)
                    ? child
                    : $"{prefix}.{child}";

                if (visited.Contains(fullKey) || fullKey.Contains('[')) continue;

                // Skip metadata sub-keys (they're checked via parent annotated key).
                bool isMeta = false;
                foreach (var s in _metaSuffixes)
                    if (fullKey.EndsWith(s, StringComparison.Ordinal)) { isMeta = true; break; }
                if (isMeta) { visited.Add(fullKey); continue; }

                var vt = reference.GetValueType(fullKey);

                if (vt == MdixValueType.Object)
                {
                    CheckMissing(locale, reference, fullKey, issues, visited, depth + 1);
                    visited.Add(fullKey);
                    continue;
                }

                if (vt == MdixValueType.Array) { visited.Add(fullKey); continue; }

                // Leaf key — check existence in locale.
                if (!locale.Exists(fullKey))
                {
                    issues.Add(new MdixLocaleIssue(
                        MdixLocaleIssueKind.MissingKey,
                        fullKey,
                        "Present in reference but absent in this locale."));
                    visited.Add(fullKey);
                    continue;
                }

                // Check for empty translation (string types only).
                if (vt == MdixValueType.String ||
                    vt == MdixValueType.Date    ||
                    vt == MdixValueType.Timestamp)
                {
                    var locStr = locale.GetString(fullKey);
                    if (locStr.IsSuccess && string.IsNullOrWhiteSpace(locStr.SuccessResult))
                    {
                        issues.Add(new MdixLocaleIssue(
                            MdixLocaleIssueKind.EmptyTranslation,
                            fullKey,
                            "Translation is empty or whitespace only."));
                    }
                }

                visited.Add(fullKey);
            }
        }
    }
}
