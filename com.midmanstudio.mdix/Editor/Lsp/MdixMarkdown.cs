using System;
using System.Text;
using MidManStudio.Mdix.Unity.Editor.Highlight;

namespace MidManStudio.Mdix.Unity.Editor.Lsp
{
    /// <summary>
    /// Converts the small Markdown subset mdix-lsp's hover text uses into
    /// Unity rich text: fenced code blocks, `inline code` and **bold**.
    /// Everything else passes through as plain text. Literal '&lt;' is
    /// neutralised (hover text is full of "&lt;int&gt;"-style generics).
    ///
    /// Pure C#, no Unity references — unit-testable.
    /// </summary>
    internal static class MdixMarkdown
    {
        private const string CodeColour = "#D7BA7D";

        public static string ToRichText(string? markdown)
        {
            if (string.IsNullOrWhiteSpace(markdown)) return string.Empty;

            var sb     = new StringBuilder(markdown!.Length + 64);
            var inCode = false;

            var lines = markdown.Replace("\r\n", "\n").Split('\n');
            for (var i = 0; i < lines.Length; i++)
            {
                var line = lines[i];

                if (line.TrimStart().StartsWith("```", StringComparison.Ordinal))
                {
                    inCode = !inCode;
                    continue; // the fence (and its language tag) is not shown
                }

                if (inCode)
                {
                    sb.Append("<color=").Append(CodeColour).Append('>')
                      .Append(MdixRichText.Escape(line))
                      .Append("</color>");
                }
                else
                {
                    AppendInline(sb, line);
                }

                if (i < lines.Length - 1) sb.Append('\n');
            }

            return sb.ToString().TrimEnd('\n', ' ');
        }

        private static void AppendInline(StringBuilder sb, string text)
        {
            var i = 0;
            while (i < text.Length)
            {
                // `code`
                if (text[i] == '`')
                {
                    var close = text.IndexOf('`', i + 1);
                    if (close > i)
                    {
                        sb.Append("<color=").Append(CodeColour).Append('>')
                          .Append(MdixRichText.Escape(text.Substring(i + 1, close - i - 1)))
                          .Append("</color>");
                        i = close + 1;
                        continue;
                    }
                }

                // **bold**
                if (text[i] == '*' && i + 1 < text.Length && text[i + 1] == '*')
                {
                    var close = text.IndexOf("**", i + 2, StringComparison.Ordinal);
                    if (close > i + 1)
                    {
                        sb.Append("<b>");
                        AppendInline(sb, text.Substring(i + 2, close - i - 2));
                        sb.Append("</b>");
                        i = close + 2;
                        continue;
                    }
                }

                if (text[i] == '<') sb.Append(MdixRichText.Escape("<"));
                else                sb.Append(text[i]);
                i++;
            }
        }
    }
}
