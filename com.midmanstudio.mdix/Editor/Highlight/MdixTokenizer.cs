using System;
using System.Collections.Generic;
using System.Text.RegularExpressions;

namespace MidManStudio.Mdix.Unity.Editor.Highlight
{
    /// <summary>
    /// Classification of a span of .mdix source text. The numeric values index
    /// <see cref="MdixPalette"/>, so keep them contiguous.
    /// </summary>
    internal enum MdixTokenClass : byte
    {
        None = 0,
        Comment,
        Section,
        String,
        StringEscape,
        Interpolation,
        Number,
        Constant,
        KeywordControl,
        KeywordDeclaration,
        TypeName,
        StaticObject,
        FunctionName,
        EnumType,
        EnumMember,
        Property,
        Operator,
    }

    internal readonly struct MdixToken
    {
        public readonly int Start;
        public readonly int Length;
        public readonly MdixTokenClass Class;

        public MdixToken(int start, int length, MdixTokenClass cls)
        {
            Start  = start;
            Length = length;
            Class  = cls;
        }

        public int End => Start + Length;
    }

    /// <summary>
    /// Instant, local syntax classification for .mdix text.
    ///
    /// This is a direct port of the TextMate grammar the VSCode extension
    /// ships (mdix-vscode/syntaxes/mdix.tmLanguage.json), so the colours land
    /// on the same spans VSCode's base highlighting does. TextMate semantics
    /// are "earliest match wins, ties broken by rule order", which is exactly
    /// how a single .NET alternation behaves, so every top-level rule becomes
    /// one named alternative in the same order as the grammar's top-level
    /// pattern list.
    ///
    /// Two deliberate deviations from the grammar, both for an interactive
    /// editor rather than a batch highlighter:
    ///   1. Date literals are tried before plain numbers. In the grammar the
    ///      numbers rule is listed first, so `2024-01-15` always loses to
    ///      `2024` and the date rule can never fire.
    ///   2. Strings are line-bounded. In the grammar an unterminated quote
    ///      keeps colouring until the next quote anywhere in the file, which
    ///      repaints the rest of the document while you are mid-keystroke.
    ///
    /// Pure C#, no UnityEngine/UnityEditor references — unit-testable.
    /// </summary>
    internal static class MdixTokenizer
    {
        /// <summary>Above this the document is not highlighted at all (keeps typing responsive).</summary>
        public const int MaxLength = 250_000;

        private const string Pattern =
            // 1. comments
            @"(?<cmt>//[^\r\n]*)" +
            @"|(?<blk>/\*[\s\S]*?(?:\*/|\z))" +
            // 2. section headers
            @"|(?<sec>@(?:CONFIG|IMPORTS|DLM|ENUMS|QUICKFUNCS|DATA|SECURITY)\b)" +
            // 3. strings (interpolated first, then double, then single quoted)
            @"|(?<istr>\$""(?:\\.|[^""\\\r\n])*(?:""|(?=[\r\n])|\z))" +
            @"|(?<dstr>""(?:\\.|[^""\\\r\n])*(?:""|(?=[\r\n])|\z))" +
            @"|(?<sstr>'(?:\\.|[^'\\\r\n])*(?:'|(?=[\r\n])|\z))" +
            // 4. prefixed constructors: t:(...) b:(...) r:(...)
            @"|(?<ctor>\b[tbr]:(?=\())" +
            // 5. hex colour literals
            @"|(?<hex>#[0-9a-fA-F]{3,8}\b)" +
            // 6. dates (before numbers — see class remarks)
            @"|(?<date>\b\d{4}-\d{2}-\d{2}(?:T\d{2}:\d{2}:\d{2}(?:Z|[+-]\d{2}:\d{2})?)?\b)" +
            // 7. numbers
            @"|(?<num>0[xX][0-9a-fA-F]+[Ll]?\b|\b\d+\.?\d*[eE][+-]?\d+\b|\b\d+\.\d+[fF]\b|\b\d+\.\d+\b|\b\d[\d_]*[Ll]\b|\b\d[\d_]*\b)" +
            // 8. constants
            @"|(?<cst>\b(?:true|false|null)\b)" +
            // 9. keywords
            @"|(?<kwc>\b(?:if|elif|else|chk|miss|return|from_cloud|from|verify|global)\b)" +
            @"|(?<kwd>\b(?:let|mut|const|log|and|or|not)\b)" +
            // 10. builtin types, only inside <...> / after a comma
            @"|(?<typ>(?<=<|,\s*)\b(?:int|long|float|double|string|bool|array|tuple|object|hex|blob|regex|date|timestamp|enum|any)\b(?=\s*[>,]))" +
            // 11. builtin static objects
            @"|(?<sto>\b(?:Math|DateTime|Array|Random|Guid|IpAddress|Enum|Dix|DCompressor|DEncryptor|DAuditor)\b)" +
            // 12. quickfunc declaration: ~name
            @"|(?<qf>~[a-zA-Z_][a-zA-Z0-9_]*)" +
            // 13. the operators that are worth a colour of their own
            @"|(?<op>->|::)" +
            // 14. identifiers: Enum.MEMBER, call(, plain
            @"|(?<enm>\b[A-Z][a-zA-Z0-9_]*\.[A-Z_][A-Z0-9_]*\b)" +
            @"|(?<fn>\b[a-zA-Z_][a-zA-Z0-9_]*(?=\s*\())" +
            @"|(?<id>\b[a-zA-Z_][a-zA-Z0-9_]*\b)";

        private static readonly string[] GroupNames =
        {
            "cmt", "blk", "sec", "istr", "dstr", "sstr", "ctor", "hex", "date", "num",
            "cst", "kwc", "kwd", "typ", "sto", "qf", "op", "enm", "fn", "id",
        };

        private static readonly Regex Matcher = new Regex(
            Pattern,
            RegexOptions.CultureInvariant,
            TimeSpan.FromMilliseconds(250));

        // group number -> kind, resolved once.
        private static readonly int[] GroupNumbers = ResolveGroupNumbers();

        private static int[] ResolveGroupNumbers()
        {
            var numbers = new int[GroupNames.Length];
            for (var i = 0; i < GroupNames.Length; i++)
                numbers[i] = Matcher.GroupNumberFromName(GroupNames[i]);
            return numbers;
        }

        /// <summary>
        /// Returns non-overlapping tokens ordered by start offset. Characters
        /// not covered by any token (whitespace, punctuation) are simply
        /// absent. Never throws: on a pathological regex timeout it returns
        /// whatever was classified so far.
        /// </summary>
        public static List<MdixToken> Tokenize(string? text)
        {
            var tokens = new List<MdixToken>();
            if (string.IsNullOrEmpty(text) || text!.Length > MaxLength)
                return tokens;

            try
            {
                var pos = 0;
                while (pos < text.Length)
                {
                    var m = Matcher.Match(text, pos);
                    if (!m.Success) break;

                    // A zero-length match would never advance; none of the
                    // alternatives can produce one, but guard anyway.
                    if (m.Length == 0)
                    {
                        pos = m.Index + 1;
                        continue;
                    }

                    var kind = -1;
                    for (var i = 0; i < GroupNumbers.Length; i++)
                    {
                        if (m.Groups[GroupNumbers[i]].Success)
                        {
                            kind = i;
                            break;
                        }
                    }

                    Emit(text, m.Index, m.Length, kind, tokens);
                    pos = m.Index + m.Length;
                }
            }
            catch (RegexMatchTimeoutException)
            {
                // Return what we have — a partially coloured file beats a frozen editor.
            }

            return tokens;
        }

        private static void Emit(string text, int start, int length, int kind, List<MdixToken> tokens)
        {
            var end = start + length;

            switch (GroupNames[kind])
            {
                case "cmt":
                case "blk":
                    tokens.Add(new MdixToken(start, length, MdixTokenClass.Comment));
                    break;

                case "sec":
                    tokens.Add(new MdixToken(start, length, MdixTokenClass.Section));
                    break;

                case "istr":
                    EmitString(text, start, end, interpolated: true, tokens);
                    break;

                case "dstr":
                case "sstr":
                    EmitString(text, start, end, interpolated: false, tokens);
                    break;

                case "ctor":
                    tokens.Add(new MdixToken(start, length, MdixTokenClass.Operator));
                    break;

                case "hex":
                case "date":
                case "num":
                    tokens.Add(new MdixToken(start, length, MdixTokenClass.Number));
                    break;

                case "cst":
                    tokens.Add(new MdixToken(start, length, MdixTokenClass.Constant));
                    break;

                case "kwc":
                    tokens.Add(new MdixToken(start, length, MdixTokenClass.KeywordControl));
                    break;

                case "kwd":
                    tokens.Add(new MdixToken(start, length, MdixTokenClass.KeywordDeclaration));
                    break;

                case "typ":
                    tokens.Add(new MdixToken(start, length, MdixTokenClass.TypeName));
                    break;

                case "sto":
                    tokens.Add(new MdixToken(start, length, MdixTokenClass.StaticObject));
                    break;

                case "qf":
                    // "~" is punctuation-coloured, the name after it is the function.
                    tokens.Add(new MdixToken(start, 1, MdixTokenClass.Operator));
                    if (length > 1)
                        tokens.Add(new MdixToken(start + 1, length - 1, MdixTokenClass.FunctionName));
                    break;

                case "op":
                    tokens.Add(new MdixToken(start, length, MdixTokenClass.Operator));
                    break;

                case "enm":
                {
                    var dot = text.IndexOf('.', start, length);
                    if (dot > start)
                    {
                        tokens.Add(new MdixToken(start, dot - start, MdixTokenClass.EnumType));
                        tokens.Add(new MdixToken(dot + 1, end - dot - 1, MdixTokenClass.EnumMember));
                    }
                    else
                    {
                        tokens.Add(new MdixToken(start, length, MdixTokenClass.EnumType));
                    }
                    break;
                }

                case "fn":
                    tokens.Add(new MdixToken(start, length, MdixTokenClass.FunctionName));
                    break;

                case "id":
                    tokens.Add(new MdixToken(start, length, MdixTokenClass.Property));
                    break;
            }
        }

        /// <summary>
        /// Splits a string literal into plain-string, escape-sequence and
        /// (for $"..." strings) {interpolation} pieces so each can carry its
        /// own colour. Pieces are contiguous and non-overlapping.
        /// </summary>
        private static void EmitString(
            string text, int start, int end, bool interpolated, List<MdixToken> tokens)
        {
            var segStart = start;
            var i = start;

            void Flush(int to)
            {
                if (to > segStart)
                    tokens.Add(new MdixToken(segStart, to - segStart, MdixTokenClass.String));
            }

            while (i < end)
            {
                var c = text[i];

                if (c == '\\' && i + 1 < end)
                {
                    Flush(i);
                    tokens.Add(new MdixToken(i, 2, MdixTokenClass.StringEscape));
                    i += 2;
                    segStart = i;
                    continue;
                }

                if (interpolated && c == '{')
                {
                    var close = text.IndexOf('}', i + 1, end - (i + 1));
                    var stop  = close >= 0 ? close + 1 : end;

                    Flush(i);
                    tokens.Add(new MdixToken(i, stop - i, MdixTokenClass.Interpolation));
                    i = stop;
                    segStart = i;
                    continue;
                }

                i++;
            }

            Flush(end);
        }
    }
}
