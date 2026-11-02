using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using MidManStudio.Mdix.Unity.Editor;

static class T
{
    public static int Pass, Fail;
    public static void Eq<X>(X want, X got, string what)
    {
        if (EqualityComparer<X>.Default.Equals(want, got)) { Pass++; return; }
        Fail++; Console.WriteLine("FAIL  " + what + "\n      want: " + Show(want) + "\n      got : " + Show(got));
    }
    public static void True(bool v, string what) => Eq(true, v, what);
    static string Show(object o) => o == null ? "null" : o is string s ? "\"" + s.Replace("\n","\\n") + "\"" : o.ToString();
}

static class Program
{
    // Oracle: the Rust lexer's escape handling (lexer.rs process_escape_sequence + scan_string_literal), written independently.
    static string RustDecodeQuoted(string lit)
    {
        var q = lit[0]; var sb = new StringBuilder(); int i = 1;
        while (i < lit.Length)
        {
            var c = lit[i];
            if (c == q) return sb.ToString();            // closing quote
            if (c == '\\')
            {
                if (i + 1 < lit.Length)
                {
                    var e = lit[i + 1];
                    sb.Append(e == 'n' ? '\n' : e == 't' ? '\t' : e == 'r' ? '\r' : e == '0' ? '\0' : e);
                    i += 2; continue;
                }
                i++; continue;
            }
            sb.Append(c); i++;
        }
        throw new Exception("unterminated: " + lit);
    }

    static Dictionary<string, MdixValueSpan> Map(string src) => MdixSourceMap.Build(src);

    static string Lit(string src, string path)
    {
        var m = Map(src);
        return m.TryGetValue(path, out var sp) ? sp.Kind + ":" + sp.TextIn(src) : "(unmapped)";
    }

    static void Main(string[] args)
    {
        // ── A. Real files ────────────────────────────────────────────────
        if (Directory.Exists("/tmp/spans_real")) Directory.Delete("/tmp/spans_real", true);
        var root = "/home/claude/w/gtg/Assets/";
        var inv = File.ReadAllText(root + "inventory_items.mdix");
        var invMap = Map(inv);
        Console.WriteLine("inventory_items.mdix spans: " + invMap.Count);
        foreach (var kv in invMap.Where(k => k.Value.Kind >= MdixLiteralKind.RawString))
            Console.WriteLine("   " + kv.Key + "  " + kv.Value.Kind + "  " + Trunc(kv.Value.TextIn(inv)));
        T.True(invMap.ContainsKey("keepSake") && invMap["keepSake"].Kind == MdixLiteralKind.RawString, "keepSake is RawString");
        T.True(inv.Substring(invMap["keepSake"].Start, 1) == "`" && inv[invMap["keepSake"].End - 1] == '`', "keepSake span includes both backticks");
        T.Eq("Blob:b:(\"\")", Lit(inv, "simpleTable.kik"), "kik blob");
        T.Eq("Regex:r:(\"\")", Lit(inv, "simpleTable.lll"), "lll regex");
        T.Eq("(unmapped)", Lit(inv, "simpleTable.aTe"), "tuple stays unmapped");

        var ps = File.ReadAllText(root + "player_save.mdix");
        T.Eq("Date:2025-01-01", Lit(ps, "last_saved"), "player_save last_saved date");
        T.Eq("String:\"Player\"", Lit(ps, "player_name"), "player_name unchanged");
        T.Eq("Int:1", Lit(ps, "level"), "level unchanged");
        T.Eq("Enum:GameDifficulty.NORMAL", Lit(ps, "difficulty"), "enum is mapped now that @ENUMS is read");

        // every real example file: Build must not throw and spans must be self-consistent
        int files = 0, spans = 0, newKinds = 0;
        foreach (var f in Directory.GetFiles("/home/claude/w/DixScript-Rust", "*.mdix", SearchOption.AllDirectories)
                     .Concat(Directory.GetFiles("/home/claude/w/gtg", "*.mdix", SearchOption.AllDirectories)))
        {
            var s = File.ReadAllText(f); files++;
            var m = Map(s);
            Directory.CreateDirectory("/tmp/spans_real");
            File.WriteAllLines("/tmp/spans_real/" + files + ".tsv", m.OrderBy(k => k.Value.Start).Select(k => k.Key + "\t" + k.Value.Kind + "\t" + Convert.ToBase64String(Encoding.UTF8.GetBytes(k.Value.TextIn(s)))));
            File.AppendAllText("/tmp/spans_real/index.txt", files + "\t" + f + "\n");
            foreach (var kv in m)
            {
                spans++;
                var sp = kv.Value;
                if (sp.Start < 0 || sp.End > s.Length || sp.Length <= 0) T.True(false, "bad span in " + f + " " + kv.Key);
                var text = sp.TextIn(s);
                if (sp.Kind >= MdixLiteralKind.RawString)
                {
                    newKinds++;
                    switch (sp.Kind)
                    {
                        case MdixLiteralKind.RawString: T.True(text.Length >= 2 && text[0]=='`' && text[^1]=='`', "raw shape " + f + " " + kv.Key); break;
                        case MdixLiteralKind.Date: T.True(MdixTemporalText.IsDateLiteral(text), "date shape " + f + " " + kv.Key); break;
                        case MdixLiteralKind.Timestamp: T.True(MdixTemporalText.IsTimestampLiteral(text), "ts shape " + f + " " + kv.Key); break;
                        case MdixLiteralKind.Regex: T.True(text.StartsWith("r:(") && MdixSourceMap.TryReadCallArgument(text, out _), "regex shape " + f + " " + kv.Key); break;
                        case MdixLiteralKind.Blob:  T.True(text.StartsWith("b:(") && MdixSourceMap.TryReadCallArgument(text, out _), "blob shape " + f + " " + kv.Key); break;
                    }
                }
            }
        }
        Console.WriteLine("scanned " + files + " .mdix files, " + spans + " spans, " + newKinds + " of the new kinds");

        // ── B. Crafted sources ───────────────────────────────────────────
        var c1 = @"@DATA(
  d = 2026-01-15
  bad = 2026-02-30
  expr = 5-3
  ts = 2025-01-15T10:30:00
  tsz = 2025-01-15T10:30:00Z
  tso = 2025-01-15T10:30:00.250+05:30
  tsneg = 2025-01-15T10:30:00-08:00
  tsfrac8 = 2025-01-15T10:30:00.12345678Z
  tsnosec = 2025-01-15T10:30
  raw = `line1
line2 ""q"" \n C:\path`
  rawopen = `never closed
)";
        T.Eq("Date:2026-01-15", Lit(c1, "d"), "date");
        T.Eq("(unmapped)", Lit(c1, "bad"), "impossible date unmapped");
        T.Eq("(unmapped)", Lit(c1, "expr"), "5-3 unmapped");
        T.Eq("Timestamp:2025-01-15T10:30:00", Lit(c1, "ts"), "ts no zone");
        T.Eq("Timestamp:2025-01-15T10:30:00Z", Lit(c1, "tsz"), "ts Z");
        T.Eq("Timestamp:2025-01-15T10:30:00.250+05:30", Lit(c1, "tso"), "ts offset+fraction");
        T.Eq("Timestamp:2025-01-15T10:30:00-08:00", Lit(c1, "tsneg"), "ts negative offset");
        T.Eq("(unmapped)", Lit(c1, "tsfrac8"), "8 fraction digits unmapped");
        T.Eq("(unmapped)", Lit(c1, "tsnosec"), "no seconds unmapped");
        T.True(Lit(c1, "raw").StartsWith("RawString:`line1\nline2"), "multiline raw mapped");
        T.Eq("(unmapped)", Lit(c1, "rawopen"), "unterminated raw unmapped");

        var c2 = @"@Data(
  a = r:(""^[a-z]+$"")
  b = r:('single')
  c = r:(`raw\d+`)
  d = b:(""aGVsbG8="")
  e = r: (""spaced"")
  f = r:(""x"").foo
  g = r:(""x"", ""y"")
  h = r:(5)
  i = t:(1, 2)
  j = x:(""no"")
  k = bb:(""no"")
  arr = [ r:(""a""), r:(""b"") ]
  obj = { p = r:(""p""), q = 2026-03-04, w = `w` }
  tab: m = 2026-05-06, n = b:(""AA=="")
)";
        T.Eq("Regex:r:(\"^[a-z]+$\")", Lit(c2, "a"), "regex dq");
        T.Eq("Regex:r:('single')", Lit(c2, "b"), "regex sq");
        T.Eq("Regex:r:(`raw\\d+`)", Lit(c2, "c"), "regex raw");
        T.Eq("Blob:b:(\"aGVsbG8=\")", Lit(c2, "d"), "blob");
        T.Eq("(unmapped)", Lit(c2, "e"), "space after colon unmapped (compiler would not read it as a constructor)");
        T.Eq("(unmapped)", Lit(c2, "f"), "member access unmapped");
        T.Eq("(unmapped)", Lit(c2, "g"), "two args unmapped");
        T.Eq("(unmapped)", Lit(c2, "h"), "non-string arg unmapped");
        T.Eq("(unmapped)", Lit(c2, "i"), "tuple unmapped");
        T.Eq("(unmapped)", Lit(c2, "j"), "unknown prefix unmapped");
        T.Eq("(unmapped)", Lit(c2, "k"), "bb: unmapped");
        T.Eq("Regex:r:(\"a\")", Lit(c2, "arr[0]"), "regex in array 0");
        T.Eq("Regex:r:(\"b\")", Lit(c2, "arr[1]"), "regex in array 1");
        T.Eq("Regex:r:(\"p\")", Lit(c2, "obj.p"), "regex in object");
        T.Eq("Date:2026-03-04", Lit(c2, "obj.q"), "date in object");
        T.Eq("RawString:`w`", Lit(c2, "obj.w"), "raw in object");
        T.Eq("Date:2026-05-06", Lit(c2, "tab.m"), "date in table");
        T.Eq("Blob:b:(\"AA==\")", Lit(c2, "tab.n"), "blob in table");

        // existing behaviour must be untouched
        var c3 = @"@DATA(
  a = 5
  b = -7L
  c = 1.5f
  d = 2.5
  e = true
  f = ""s""
  g = #F80
  h = 0xFF
  i = fn(1, 2)
  j = 1 + 2
)";
        T.Eq("Int:5", Lit(c3, "a"), "int"); T.Eq("Long:-7L", Lit(c3, "b"), "long"); T.Eq("Float:1.5f", Lit(c3, "c"), "float");
        T.Eq("Double:2.5", Lit(c3, "d"), "double"); T.Eq("Bool:true", Lit(c3, "e"), "bool"); T.Eq("String:\"s\"", Lit(c3, "f"), "string");
        T.Eq("HexColor:#F80", Lit(c3, "g"), "colour"); T.Eq("Int:0xFF", Lit(c3, "h"), "0xFF is a mapped int now"); T.Eq("(unmapped)", Lit(c3, "i"), "call unmapped"); T.Eq("(unmapped)", Lit(c3, "j"), "expr unmapped");

        // ── C. TryFormat ─────────────────────────────────────────────────
        string lit, exp, err;
        bool F(MdixLiteralKind k, string old, string input) => MdixLiteralEdit.TryFormat(k, old, input, out lit, out exp, out err);

        T.True(F(MdixLiteralKind.Date, "2026-01-15", " 2026-1-5 "), "date loose ok"); T.Eq("2026-01-05", lit, "date normalised");
        T.True(!F(MdixLiteralKind.Date, "2026-01-15", "2026-02-30"), "date impossible refused");
        T.True(!F(MdixLiteralKind.Date, "2026-01-15", "26-01-15"), "date 2-digit year refused");
        T.True(!F(MdixLiteralKind.Date, "2026-01-15", ""), "date empty refused");

        T.True(F(MdixLiteralKind.Timestamp, "x", "2025-01-15 10:30:00.500 +05:30"), "ts loose ok"); T.Eq("2025-01-15T10:30:00.500+05:30", lit, "ts normalised keeps offset + ms");
        T.True(F(MdixLiteralKind.Timestamp, "x", "2025-01-15T10:30"), "ts no seconds ok"); T.Eq("2025-01-15T10:30:00", lit, "seconds added");
        T.True(F(MdixLiteralKind.Timestamp, "x", "2025-01-15T10:30:00z"), "ts lowercase z"); T.Eq("2025-01-15T10:30:00Z", lit, "z -> Z");
        T.True(F(MdixLiteralKind.Timestamp, "x", "2025-01-15T10:30:00+0530"), "ts +hhmm"); T.Eq("2025-01-15T10:30:00+05:30", lit, "+hhmm normalised");
        T.True(F(MdixLiteralKind.Timestamp, "x", "2025-01-15T10:30:00+05"), "ts +hh"); T.Eq("2025-01-15T10:30:00+05:00", lit, "+hh normalised");
        T.True(!F(MdixLiteralKind.Timestamp, "x", "2025-01-15T24:00:00"), "hour 24 refused");
        T.True(!F(MdixLiteralKind.Timestamp, "x", "2025-01-15T10:60:00"), "minute 60 refused");
        T.True(!F(MdixLiteralKind.Timestamp, "x", "2025-01-15T10:30:00+15:00"), "offset > 14:00 refused");
        T.True(!F(MdixLiteralKind.Timestamp, "x", "2025-01-15T10:30:00.12345678"), "8 fraction digits refused");
        T.True(!F(MdixLiteralKind.Timestamp, "x", "2025-01-15"), "date only refused for timestamp");
        T.True(F(MdixLiteralKind.Timestamp, "x", "2025-01-15T10:30:00.1234567-14:00"), "7 digits and -14:00 ok");

        // raw string
        T.True(F(MdixLiteralKind.RawString, "`a`", "he said \"hi\"\nC:\\x"), "raw ok"); T.Eq("`he said \"hi\"\nC:\\x`", lit, "raw verbatim");
        T.True(!F(MdixLiteralKind.RawString, "`a`", "has ` tick"), "raw with backtick refused");

        // regex: raw stays raw when possible, quoted escapes backslashes
        T.True(F(MdixLiteralKind.Regex, "r:(\"x\")", @"^\d+\w$"), "regex ok"); T.Eq("r:(\"^\\\\d+\\\\w$\")", lit, "dq regex doubles backslashes");
        T.True(F(MdixLiteralKind.Regex, "r:('x')", @"it's \d"), "regex sq ok"); T.Eq("r:('it\\'s \\\\d')", lit, "sq regex escapes quote and backslash");
        T.True(F(MdixLiteralKind.Regex, "r:(`x`)", @"^\d+$"), "regex raw ok"); T.Eq("r:(`^\\d+$`)", lit, "raw regex stays raw");
        T.True(F(MdixLiteralKind.Regex, "r:(`x`)", "a`b"), "raw regex with backtick falls back"); T.Eq("r:(\"a`b\")", lit, "falls back to quoted");
        T.True(F(MdixLiteralKind.Regex, "r:(\"x\")", ""), "empty regex ok"); T.Eq("r:(\"\")", lit, "empty regex");

        // blob
        T.True(F(MdixLiteralKind.Blob, "b:(\"\")", "aGVs bG8="), "blob with space ok"); T.Eq("b:(\"aGVsbG8=\")", lit, "blob canonical");
        T.Eq(MdixBlobInfo.Fingerprint("aGVsbG8="), exp, "blob expected = fingerprint");
        T.True(!F(MdixLiteralKind.Blob, "b:(\"\")", "not base64!"), "blob invalid refused");
        T.True(F(MdixLiteralKind.Blob, "b:('')", ""), "empty blob ok"); T.Eq("b:('')", lit, "blob keeps single quotes");

        // existing string behaviour unchanged
        T.True(F(MdixLiteralKind.String, "\"a\"", "a\"b\\c\n\t"), "string ok"); T.Eq("\"a\\\"b\\\\c\\n\\t\"", lit, "string escapes unchanged");
        T.True(F(MdixLiteralKind.Int, "5", "42"), "int ok"); T.Eq("42", lit, "int");

        // ── D. Escaping round trip against the independent oracle ────────
        var rng = new Random(12345);
        var alphabet = "abcXYZ019 _.-\\\"'`{}()[]$^*+?|/\t\n\r:,;é世😀";
        int rt = 0;
        for (int n = 0; n < 4000; n++)
        {
            var len = rng.Next(0, 14);
            var sb = new StringBuilder();
            for (int i = 0; i < len; i++) sb.Append(alphabet[rng.Next(alphabet.Length)]);
            var value = sb.ToString();
            if (value.Contains('\0')) continue;

            foreach (var q in new[] { '"', '\'' })
            {
                var old = q + "x" + q;
                MdixLiteralEdit.TryFormat(MdixLiteralKind.String, old, value, out var l, out _, out _);
                T.Eq(value, RustDecodeQuoted(l), "oracle round trip (string) " + l);
                T.Eq(value, MdixSourceMap.DecodeString(l), "DecodeString agrees " + l);
                // the written literal must be a single string token that the source map maps as the whole value
                if (!value.Contains('\n') && !value.Contains('\r'))
                    T.Eq("String:" + l, Lit("@DATA(\n v = " + l + "\n)", "v"), "mapped as one literal " + l);

                var oldCall = "r:(" + old + ")";
                MdixLiteralEdit.TryFormat(MdixLiteralKind.Regex, oldCall, value, out var rl, out _, out _);
                MdixSourceMap.TrySplitCall(rl, out var arg);
                T.Eq(value, RustDecodeQuoted(arg), "oracle round trip (regex) " + rl);
                T.True(MdixSourceMap.TryReadCallArgument(rl, out var back) && back == value, "TryReadCallArgument agrees " + rl);
                if (!value.Contains('\n') && !value.Contains('\r'))
                    T.Eq("Regex:" + rl, Lit("@DATA(\n v = " + rl + "\n)", "v"), "regex mapped whole " + rl);
            }
            rt++;
        }
        Console.WriteLine("escape round trips: " + rt + " random values x 2 quote styles x 2 kinds");

        // ── E. Temporal text ─────────────────────────────────────────────
        T.True(MdixTemporalText.IsDateLiteral("2024-02-29"), "leap day ok");
        T.True(!MdixTemporalText.IsDateLiteral("2023-02-29"), "non-leap Feb 29 refused");
        T.True(!MdixTemporalText.IsDateLiteral("2026-1-05"), "strict needs 2-digit month");
        T.True(!MdixTemporalText.IsDateLiteral("0000-01-01"), "year 0 refused");
        T.True(MdixTemporalText.IsTimestampLiteral("2025-01-15T10:30:00.1Z"), "ts literal with fraction + Z");
        T.True(!MdixTemporalText.IsTimestampLiteral("2025-01-15T10:30:00-00:00"), "-00:00 not canonical -> not mapped");
        foreach (var s in new[] { "2025-01-15T00:00:00", "2025-12-31T23:59:59.9999999+14:00", "1999-06-07T08:09:10-03:30", "2025-01-15T10:30:00Z" })
        {
            T.True(MdixTemporalText.TryParseTimestamp(s, true, out var p, out _), "parse " + s);
            T.Eq(s, MdixTemporalText.FormatTimestamp(p), "round trip " + s);
        }
        T.Eq(0, MdixTemporalText.WeekdayMondayFirst(2026, 1, 12), "2026-01-12 is a Monday");
        T.Eq(6, MdixTemporalText.WeekdayMondayFirst(2026, 1, 18), "2026-01-18 is a Sunday");
        T.Eq("+05:30", MdixTemporalText.FormatOffset(330), "offset fmt"); T.Eq("-08:00", MdixTemporalText.FormatOffset(-480), "neg offset fmt");

        // does the runtime accept what we write? (MdixTimestamp / MdixDate parse, the same calls GetTimestamp / GetDate make)
        foreach (var s in new[] { "2025-01-15T10:30:00", "2025-01-15T10:30:00Z", "2025-01-15T10:30:00.250+05:30", "2025-01-15T10:30:00.1234567-14:00", "2025-12-31T23:59:59.9999999+14:00" })
            T.True(MidManStudio.Mdix.Core.MdixTimestamp.Parse(s).IsSuccess, "runtime parses timestamp " + s);
        T.True(MidManStudio.Mdix.Core.MdixDate.Parse("2026-01-05").IsSuccess, "runtime parses date");
        T.True(MidManStudio.Mdix.Core.MdixDate.Parse("2026-02-30").IsFailure, "runtime rejects 2026-02-30 (so refusing it is right)");

        // ── F. Blob info ─────────────────────────────────────────────────
        byte[] Hdr(params byte[] b) { var x = new byte[64]; Array.Copy(b, x, b.Length); return x; }
        MdixBlobKind K(byte[] b) => MdixBlobInfo.Sniff(b, b.Length);
        T.Eq(MdixBlobKind.Png,  K(Hdr(0x89,0x50,0x4E,0x47,0x0D,0x0A,0x1A,0x0A)), "png");
        T.Eq(MdixBlobKind.Jpeg, K(Hdr(0xFF,0xD8,0xFF,0xE0)), "jpeg");
        T.Eq(MdixBlobKind.Gif,  K(Encoding.ASCII.GetBytes("GIF89a....")), "gif");
        T.Eq(MdixBlobKind.Bmp,  K(Hdr(0x42,0x4D,1,2,3,4,0,0,0,0,1,1,1,1)), "bmp");
        T.Eq(MdixBlobKind.Text, K(Encoding.ASCII.GetBytes("BMW is not a bitmap, just text that starts with BM")), "BM text is text not bmp");
        var riff = new byte[20]; Encoding.ASCII.GetBytes("RIFF").CopyTo(riff,0); Encoding.ASCII.GetBytes("WAVE").CopyTo(riff,8);
        T.Eq(MdixBlobKind.Wav, K(riff), "wav");
        Encoding.ASCII.GetBytes("WEBP").CopyTo(riff,8); T.Eq(MdixBlobKind.Webp, K(riff), "webp");
        T.Eq(MdixBlobKind.Ogg, K(Encoding.ASCII.GetBytes("OggS\0\x02")), "ogg");
        T.Eq(MdixBlobKind.Flac, K(Encoding.ASCII.GetBytes("fLaC\0\0")), "flac");
        T.Eq(MdixBlobKind.Mp3, K(Encoding.ASCII.GetBytes("ID3\x03\0\0")), "mp3 id3");
        T.Eq(MdixBlobKind.Mp3, K(Hdr(0xFF,0xFB,0x90)), "mp3 frame sync");
        T.Eq(MdixBlobKind.WebM, K(Hdr(0x1A,0x45,0xDF,0xA3)), "webm");
        var ftyp = new byte[16]; Encoding.ASCII.GetBytes("ftypM4A ").CopyTo(ftyp,4); T.Eq(MdixBlobKind.M4a, K(ftyp), "m4a");
        Encoding.ASCII.GetBytes("ftypisom").CopyTo(ftyp,4); T.Eq(MdixBlobKind.Mp4, K(ftyp), "mp4");
        T.Eq(MdixBlobKind.Pdf, K(Encoding.ASCII.GetBytes("%PDF-1.7")), "pdf");
        T.Eq(MdixBlobKind.Text, K(Encoding.UTF8.GetBytes("hello world\nline two\n")), "text");
        T.Eq(MdixBlobKind.Unknown, K(new byte[] { 0, 1, 2, 3, 4, 5, 6, 7, 200, 201, 202, 203 }), "binary");
        T.Eq(MdixBlobKind.Unknown, MdixBlobInfo.Sniff(new byte[0], 0), "empty is unknown");

        string B64(byte[] b) => Convert.ToBase64String(b);
        T.Eq("blob  empty", MdixBlobInfo.Summary(""), "empty summary");
        T.Eq("blob  invalid base64", MdixBlobInfo.Summary("abc"), "bad summary");
        var png = new byte[4300]; new byte[] {0x89,0x50,0x4E,0x47,0x0D,0x0A,0x1A,0x0A}.CopyTo(png, 0);
        T.Eq("blob  4.2 KB  PNG", MdixBlobInfo.Summary(B64(png)), "png summary");
        T.Eq("blob  5 B  text", MdixBlobInfo.Summary(B64(Encoding.ASCII.GetBytes("hello"))), "tiny text summary");
        foreach (var n in new[] { 1, 2, 3, 4, 5, 100, 511, 512, 513, 1000, 100000 })
        {
            var data = new byte[n]; rng.NextBytes(data);
            var b64 = B64(data);
            T.Eq((long)n, MdixBlobInfo.DecodedLength(b64), "decoded length " + n);
            var head = MdixBlobInfo.HeadBytes(b64, 512);
            T.True(head.SequenceEqual(data.Take(512)), "head bytes " + n);
        }
        T.Eq("0 B", MdixBlobInfo.FormatSize(0), "size 0"); T.Eq("1023 B", MdixBlobInfo.FormatSize(1023), "size 1023");
        T.Eq("1.0 KB", MdixBlobInfo.FormatSize(1024), "size 1K"); T.Eq("1.0 MB", MdixBlobInfo.FormatSize(1048576), "size 1M");
        T.True(MdixBlobInfo.Fingerprint("abc") != MdixBlobInfo.Fingerprint("abd"), "fingerprint differs");
        T.Eq(MdixBlobInfo.Fingerprint("abc"), MdixBlobInfo.Fingerprint("abc"), "fingerprint stable");

        // ── G. Date/time model ───────────────────────────────────────────
        var now = new DateTime(2026, 10, 27, 14, 5, 9, DateTimeKind.Local);
        var utcNow = new DateTime(2026, 10, 27, 12, 5, 9, DateTimeKind.Utc);
        string e2;

        var dm = new MdixDateTimeModel(false, "2026-01-15", now);
        T.Eq("2026-01-15", dm.Text, "date model text");
        T.Eq(2026, dm.ViewYear, "view year"); T.Eq(1, dm.ViewMonth, "view month");
        var cells = dm.Cells();
        T.Eq(42, cells.Length, "42 cells");
        T.True(cells[3].Day == 1 && cells[3].InViewMonth && cells[3].Month == 1, "Jan 1 2026 is a Thursday: column 3");
        T.True(cells[0].Day == 29 && cells[0].Month == 12 && cells[0].Year == 2025 && !cells[0].InViewMonth, "grid starts on Dec 29 2025");
        T.True(cells[33].Day == 31 && cells[33].InViewMonth && !cells[34].InViewMonth && cells[34].Day == 1 && cells[34].Month == 2, "Jan ends at cell 33, Feb 1 follows");
        T.True(dm.IsSelected(cells[17]) && cells[17].Day == 15, "Jan 15 is cell 17 and selected");
        int inView = 0; foreach (var c in cells) if (c.InViewMonth) inView++;
        T.Eq(31, inView, "31 days in view");

        dm.MoveMonth(1);  T.Eq("2026-01-15", dm.Text, "moving the view keeps the value"); T.True(dm.ViewYear == 2026 && dm.ViewMonth == 2, "next month");
        dm.MoveMonth(-3); T.True(dm.ViewYear == 2025 && dm.ViewMonth == 11, "back three months wraps the year");
        dm.MoveYear(2);   T.True(dm.ViewYear == 2027 && dm.ViewMonth == 11, "two years on");
        dm.TrySetText("2024-02-10", out e2); inView = 0; foreach (var c in dm.Cells()) if (c.InViewMonth) inView++;
        T.Eq(29, inView, "Feb 2024 has 29 days");
        dm.MoveMonth(1); var mar = dm.Cells(); T.True(mar[0].Month == 2 || mar[0].Month == 3, "march grid ok");

        var lo = new MdixDateTimeModel(false, "0001-01-15", now);
        T.Eq(0, lo.Cells()[0].Day == 1 ? 0 : 1, "0001-01-01 (a Monday) is cell 0");
        lo.MoveMonth(-5); T.True(lo.ViewYear == 1 && lo.ViewMonth == 1, "clamped at 0001-01");
        lo.TrySetText("0001-02-10", out e2);
        T.True(lo.Cells()[0].Exists && lo.Cells()[0].Month == 1 && lo.Cells()[0].Day == 29, "0001-02 grid shows Jan 29 of year 1");
        var hi = new MdixDateTimeModel(false, "9999-12-20", now);
        hi.MoveMonth(+9); T.True(hi.ViewYear == 9999 && hi.ViewMonth == 12, "clamped at 9999-12");
        var hc = hi.Cells(); int missing = 0; foreach (var c in hc) if (!c.Exists) missing++;
        T.True(missing > 0 && hc[41].Day == 0, "cells past 9999-12-31 do not exist");
        T.True(hc.Where(c => c.Exists && c.InViewMonth).Count() == 31, "9999-12 still shows its 31 days");

        dm = new MdixDateTimeModel(false, "2026-01-15", now);
        dm.SelectDay(2026, 2, 1); T.Eq("2026-02-01", dm.Text, "select a day"); T.True(dm.ViewMonth == 2, "view follows the selected day");
        dm.SelectDay(2025, 12, 29); T.True(dm.ViewYear == 2025 && dm.ViewMonth == 12, "picking a grey day moves the view to its month");

        T.True(dm.TrySetText(" 2027-3-4 ", out e2) && dm.Text == "2027-03-04", "typed loose date normalised");
        T.True(dm.ViewYear == 2027 && dm.ViewMonth == 3, "view follows typed date");
        T.True(!dm.TrySetText("2027-02-30", out e2) && e2 != null && dm.Text == "2027-03-04", "bad date refused, value unchanged");
        dm.SetNow(utcNow, now); T.Eq("2026-10-27", dm.Text, "today");

        var badDate = new MdixDateTimeModel(false, "not a date", now);
        T.Eq("2026-10-27", badDate.Text, "unreadable start falls back to today");

        var tm = new MdixDateTimeModel(true, "2025-01-15T10:30:00.250+05:30", now);
        T.Eq("2025-01-15T10:30:00.250+05:30", tm.Text, "timestamp text kept as written");
        T.True(tm.TrySetTime("7", "5", "9", "12", out e2), "set time");           T.Eq("2025-01-15T07:05:09.12+05:30", tm.Text, "time changed, zone and offset kept");
        T.True(tm.TrySetTime("07", "05", "09", "", out e2), "clear fraction");    T.Eq("2025-01-15T07:05:09+05:30", tm.Text, "fraction cleared");
        T.True(!tm.TrySetTime("25", "0", "0", "", out e2) && e2 != null, "hour 25 refused");        T.Eq("2025-01-15T07:05:09+05:30", tm.Text, "unchanged after refused time");
        T.True(!tm.TrySetTime("1", "2", "3", "12345678", out e2), "8 fraction digits refused");
        T.True(!tm.TrySetTime("1", "", "3", "", out e2), "empty minute refused");
        tm.SetZone(MdixZoneKind.Utc);  T.Eq("2025-01-15T07:05:09Z", tm.Text, "UTC");
        tm.SetZone(MdixZoneKind.None); T.Eq("2025-01-15T07:05:09", tm.Text, "no zone");
        tm.SetZone(MdixZoneKind.Offset); T.Eq("2025-01-15T07:05:09+00:00", tm.Text, "offset starts at +00:00");
        T.True(tm.TrySetOffsetText("-08:00", out e2), "offset -08:00");        T.Eq("2025-01-15T07:05:09-08:00", tm.Text, "negative offset");
        tm.SetZone(MdixZoneKind.Offset);                                         T.Eq("2025-01-15T07:05:09-08:00", tm.Text, "choosing Offset again keeps it");
        T.True(!tm.TrySetOffsetText("+15:00", out e2) && e2 != null, "offset beyond 14:00 refused"); T.Eq("-08:00", tm.OffsetText, "offset unchanged");
        T.True(!tm.TrySetOffsetText("", out e2), "empty offset refused");
        T.True(tm.TrySetOffsetText("+0530", out e2) && tm.OffsetText == "+05:30", "offset +0530");

        tm.SetZone(MdixZoneKind.None); tm.SetNow(utcNow, now);   T.Eq("2026-10-27T14:05:09", tm.Text, "now, no zone = local");
        tm.SetZone(MdixZoneKind.Utc);  tm.SetNow(utcNow, now);   T.Eq("2026-10-27T12:05:09Z", tm.Text, "now in UTC");
        tm.SetZone(MdixZoneKind.Offset); tm.TrySetOffsetText("+05:30", out e2); tm.SetNow(utcNow, now); T.Eq("2026-10-27T17:35:09+05:30", tm.Text, "now in +05:30");
        tm.TrySetOffsetText("+13:00", out e2); tm.SetNow(new DateTime(2026,10,27,23,0,0,DateTimeKind.Utc), now); T.Eq("2026-10-28T12:00:00+13:00", tm.Text, "now crosses midnight into next day");

        var badTs = new MdixDateTimeModel(true, "garbage", now); T.Eq("2026-10-27T14:05:09", badTs.Text, "unreadable timestamp falls back to now");

        // what the picker writes must be what the edit pipeline accepts and leaves alone
        foreach (var text in new[] { "2025-01-15T07:05:09.12+05:30", "2025-01-15T07:05:09Z", "2025-01-15T07:05:09" })
        {
            T.True(MdixLiteralEdit.TryFormat(MdixLiteralKind.Timestamp, "x", text, out var l2, out var x2, out var er2) && l2 == text && x2 == text, "pipeline keeps picker text " + text);
        }
        T.True(MdixLiteralEdit.TryFormat(MdixLiteralKind.Date, "x", "2026-02-01", out var l3, out _, out _) && l3 == "2026-02-01", "pipeline keeps picker date");

        // ── H. Regex test layer ──────────────────────────────────────────
        // request
        var rp = MdixRegexTest.BuildParams("a\\d\"q\n", "x é 世 😀");
        var rpBack = MidManStudio.Mdix.Unity.Editor.Lsp.MdixJsonValue.Parse(rp.ToString());
        T.Eq("mdix.testRegex", rpBack["command"].AsString(), "request command");
        T.Eq("a\\d\"q\n", rpBack["arguments"][0].AsString(), "request pattern survives JSON");
        T.Eq("x é 世 😀", rpBack["arguments"][1].AsString(), "request text survives JSON");
        T.Eq(2, rpBack["arguments"].Count, "two arguments");

        // byte offsets -> char indexes, against Encoding.UTF8 as the oracle
        var rr = new Random(777);
        var rAlphabet = new[] { "a", "Z", " ", "é", "ñ", "世", "界", "€", "😀", "𝄞", "\n", "\t" };
        int rbOk = 0;
        for (int n = 0; n < 4000; n++)
        {
            var sbr = new StringBuilder(); int pieces = rr.Next(0, 12);
            for (int i = 0; i < pieces; i++) sbr.Append(rAlphabet[rr.Next(rAlphabet.Length)]);
            var txt = sbr.ToString();
            for (int i = 0; i <= txt.Length; i++)
            {
                if (i > 0 && i < txt.Length && char.IsLowSurrogate(txt[i])) continue;       // not a boundary
                var byteOff = Encoding.UTF8.GetByteCount(txt.Substring(0, i));
                if (MdixRegexTest.ByteToCharIndex(txt, byteOff) != i) { T.True(false, "byte->char " + txt + " @" + i); break; }
                rbOk++;
            }
        }
        T.True(rbOk > 20000, "byte->char boundaries checked: " + rbOk);
        T.Eq(0, MdixRegexTest.ByteToCharIndex("abc", -5), "negative offset clamps");
        T.Eq(3, MdixRegexTest.ByteToCharIndex("abc", 99), "past the end clamps to length");
        T.Eq(0, MdixRegexTest.ByteToCharIndex("", 4), "empty text");

        // the server's answer, in the shape serde_json writes for RegexTestResult
        string ServerJson(string text, string needle, bool withGroups)
        {
            var ms = new List<string>(); int from = 0;
            while (true)
            {
                var ix = text.IndexOf(needle, from, StringComparison.Ordinal); if (ix < 0) break;
                var b0 = Encoding.UTF8.GetByteCount(text.Substring(0, ix));
                var b1 = b0 + Encoding.UTF8.GetByteCount(needle);
                var groups = withGroups ? "[{\"name\":null,\"value\":\"x\"},{\"name\":\"year\",\"value\":null}]" : "[]";
                ms.Add("{\"start\":" + b0 + ",\"end\":" + b1 + ",\"text\":\"" + needle + "\",\"groups\":" + groups + "}");
                from = ix + Math.Max(1, needle.Length);
            }
            return "{\"valid\":true,\"error\":null,\"matches\":[" + string.Join(",", ms) + "]}";
        }
        var rtext = "café 世界 😀 ok ok 😀ok";
        var srv = MdixRegexTest.FromServer(MidManStudio.Mdix.Unity.Editor.Lsp.MdixJsonValue.Parse(ServerJson(rtext, "ok", true)), rtext);
        T.True(srv != null && srv.Valid && !srv.Approximate && srv.Error == null, "server result valid, not approximate");
        T.Eq(3, srv.Hits.Count, "three matches");
        T.Eq(rtext.IndexOf("ok"), srv.Hits[0].Start, "first match start in chars (after accents, CJK, emoji)");
        T.Eq(rtext.IndexOf("ok", rtext.IndexOf("ok") + 2), srv.Hits[1].Start, "second match start");
        T.Eq(rtext.LastIndexOf("ok"), srv.Hits[2].Start, "match after an emoji");
        foreach (var h in srv.Hits) T.Eq("ok", rtext.Substring(h.Start, h.Length), "range selects the match text");
        T.Eq(2, srv.Hits[0].Groups.Count, "two groups"); T.True(srv.Hits[0].Groups[0].Name == null && srv.Hits[0].Groups[0].Value == "x", "unnamed group with a value");
        T.True(srv.Hits[0].Groups[1].Name == "year" && srv.Hits[0].Groups[1].Value == null, "named group that did not take part");
        T.Eq("Valid. 3 matches.", MdixRegexTest.Summary(srv), "summary");

        var none = MdixRegexTest.FromServer(MidManStudio.Mdix.Unity.Editor.Lsp.MdixJsonValue.Parse("{\"valid\":true,\"error\":null,\"matches\":[]}"), "abc");
        T.True(none.Valid && none.Hits.Count == 0, "valid, no matches"); T.Eq("Valid. No matches.", MdixRegexTest.Summary(none), "no matches summary");
        var one = MdixRegexTest.FromServer(MidManStudio.Mdix.Unity.Editor.Lsp.MdixJsonValue.Parse(ServerJson("ab", "a", false)), "ab");
        T.Eq("Valid. 1 match.", MdixRegexTest.Summary(one), "one match summary");

        var bad = MdixRegexTest.FromServer(MidManStudio.Mdix.Unity.Editor.Lsp.MdixJsonValue.Parse(
            "{\"valid\":false,\"error\":\"regex parse error:\\n    [unclosed\\n    ^\\nerror: unclosed character class\",\"matches\":[]}"), "abc");
        T.True(bad != null && !bad.Valid && !bad.Approximate, "invalid server result"); T.Eq("unclosed character class", bad.Error, "rust parse error reduced to its last line");
        T.Eq("", MdixRegexTest.Summary(bad), "no summary for invalid");
        T.Eq("unclosed group", MdixRegexTest.FriendlyError("regex parse error:\n    (a\n    ^\nerror: unclosed group"), "friendly: unclosed group");
        T.Eq("Compiled regex exceeds size limit of 10485760 bytes.", MdixRegexTest.FriendlyError("Compiled regex exceeds size limit of 10485760 bytes."), "friendly: one line unchanged");
        T.Eq("The pattern is not valid.", MdixRegexTest.FriendlyError("  \n "), "friendly: blank");
        T.Eq("Invalid pattern 'x' at offset 1. Unterminated [] set.", MdixRegexTest.FriendlyError("Invalid pattern 'x' at offset 1. Unterminated [] set.\r\nmore"), ".NET style: first line");

        T.True(MdixRegexTest.FromServer(MidManStudio.Mdix.Unity.Editor.Lsp.MdixJsonValue.Parse("{}"), "x") == null, "empty object is not an answer");
        T.True(MdixRegexTest.FromServer(MidManStudio.Mdix.Unity.Editor.Lsp.MdixJsonValue.Null, "x") == null, "null is not an answer");
        T.True(MdixRegexTest.FromServer(MidManStudio.Mdix.Unity.Editor.Lsp.MdixJsonValue.Parse("[1]"), "x") == null, "array is not an answer");
        T.True(MdixRegexTest.FromServer(null, "x") == null, "no json is not an answer");

        var manyText = new string('a', 600);
        var manyMs = string.Join(",", Enumerable.Range(0, 600).Select(i => "{\"start\":" + i + ",\"end\":" + (i + 1) + ",\"text\":\"a\",\"groups\":[]}"));
        var capped = MdixRegexTest.FromServer(MidManStudio.Mdix.Unity.Editor.Lsp.MdixJsonValue.Parse("{\"valid\":true,\"error\":null,\"matches\":[" + manyMs + "]}"), manyText);
        T.True(capped.HitsCapped && capped.Hits.Count == MdixRegexTest.MaxHits, "hits capped at 500");
        T.True(MdixRegexTest.Summary(capped).Contains("500+"), "capped summary says 500+");

        // empty matches (a pattern like a*) keep their place and have zero length
        var emp = MdixRegexTest.FromServer(MidManStudio.Mdix.Unity.Editor.Lsp.MdixJsonValue.Parse(
            "{\"valid\":true,\"error\":null,\"matches\":[{\"start\":0,\"end\":0,\"text\":\"\",\"groups\":[]},{\"start\":1,\"end\":1,\"text\":\"\",\"groups\":[]}]}"), "b");
        T.True(emp.Hits.Count == 2 && emp.Hits[0].Length == 0 && emp.Hits[1].Start == 1, "empty matches are kept with zero length");

        // the .NET fallback
        var dn = MdixRegexTest.FromDotNet(@"(?<year>\d{4})-(\d{2})", "on 2026-01 and 1999-12");
        T.True(dn.Valid && dn.Approximate && dn.Hits.Count == 2, ".NET: valid, approximate, two matches");
        T.Eq("2026-01", dn.Hits[0].Text, ".NET: first match text"); T.Eq(3, dn.Hits[0].Start, ".NET: start");
        var dnOpt = MdixRegexTest.FromDotNet("(a)|(b)", "b");
        T.True(dnOpt.Hits[0].Groups[0].Value == null && dnOpt.Hits[0].Groups[1].Value == "b", ".NET: group that did not take part is null");
        T.True(dnOpt.Hits[0].Groups[0].Name == null, ".NET: numbered group reports no name");
        var dnBad = MdixRegexTest.FromDotNet("[unclosed", "x");
        T.True(!dnBad.Valid && dnBad.Approximate && !string.IsNullOrEmpty(dnBad.Error), ".NET: invalid pattern reported");
        T.True(MdixRegexTest.FromDotNet("a(?=b)", "ab").Valid, ".NET accepts look-ahead (the Rust crate does not): why it is labelled approximate");
        T.True(MdixRegexTest.FromDotNet("", "ab").Valid, ".NET: empty pattern is valid");
        var sw = System.Diagnostics.Stopwatch.StartNew();
        var slow = MdixRegexTest.FromDotNet("(a+)+$", new string('a', 45) + "!");
        sw.Stop();
        T.True(!slow.Valid && slow.Error.Contains("too long") && sw.ElapsedMilliseconds < 5000, ".NET: catastrophic pattern times out (" + sw.ElapsedMilliseconds + " ms)");
        var dn600 = MdixRegexTest.FromDotNet("a", new string('a', 600)); T.True(dn600.HitsCapped && dn600.Hits.Count == 500, ".NET: hits capped");

        // text for the list
        T.Eq("a\\nb\\tc\\r", MdixRegexTest.Visible("a\nb\tc\r"), "visible escapes control characters");
        T.Eq(new string('x', 5) + "…", MdixRegexTest.Visible(new string('x', 20), 5), "visible truncates");
        T.Eq("#2  7–10  \"abc\"", MdixRegexTest.DescribeHit(2, new MdixRegexHit { Start = 7, Length = 3, Text = "abc" }), "describe hit");
        T.Eq("1: \"x\"", MdixRegexTest.DescribeGroup(1, new MdixRegexGroupInfo { Value = "x" }), "describe unnamed group");
        T.Eq("year: \"2026\"", MdixRegexTest.DescribeGroup(2, new MdixRegexGroupInfo { Name = "year", Value = "2026" }), "describe named group");
        T.Eq("2: (no match)", MdixRegexTest.DescribeGroup(2, new MdixRegexGroupInfo { Value = null }), "describe group that did not match");

        // ── I. Blob viewer helpers ───────────────────────────────────────
        T.Eq("PNG image", MdixBlobInfo.LongLabel(MdixBlobKind.Png), "label png"); T.Eq("Binary data", MdixBlobInfo.LongLabel(MdixBlobKind.Unknown), "label unknown");
        T.Eq("Ogg audio", MdixBlobInfo.LongLabel(MdixBlobKind.Ogg), "label ogg"); T.Eq("Text", MdixBlobInfo.LongLabel(MdixBlobKind.Text), "label text");
        foreach (MdixBlobKind k in Enum.GetValues(typeof(MdixBlobKind)))
        {
            T.True(MdixBlobInfo.Extension(k).StartsWith(".") && MdixBlobInfo.Extension(k).Length >= 4, "extension has a dot: " + k);
            T.True(MdixBlobInfo.LongLabel(k).Length > 0 && MdixBlobInfo.ShortLabel(k).Length > 0, "labels exist: " + k);
        }
        T.Eq(".jpg", MdixBlobInfo.Extension(MdixBlobKind.Jpeg), "jpeg ext"); T.Eq(".bin", MdixBlobInfo.Extension(MdixBlobKind.Unknown), "unknown ext");
        T.True(MdixBlobInfo.CanPreviewImage(MdixBlobKind.Png) && MdixBlobInfo.CanPreviewImage(MdixBlobKind.Jpeg) && !MdixBlobInfo.CanPreviewImage(MdixBlobKind.Gif) && !MdixBlobInfo.CanPreviewImage(MdixBlobKind.Webp), "image preview kinds");
        T.True(MdixBlobInfo.CanPlay(MdixBlobKind.Wav) && MdixBlobInfo.CanPlay(MdixBlobKind.Ogg) && !MdixBlobInfo.CanPlay(MdixBlobKind.Mp3) && !MdixBlobInfo.CanPlay(MdixBlobKind.Flac), "playable kinds");

        T.True(MdixBlobInfo.TryDecode("aGVsbG8=", out var dec) && Encoding.ASCII.GetString(dec) == "hello", "decode ok");
        T.True(!MdixBlobInfo.TryDecode("not base64!", out var dec2) && dec2 == null, "decode refuses bad base64");
        T.True(MdixBlobInfo.TryDecode("", out var dec3) && dec3.Length == 0, "decode empty");

        T.Eq("kik.png", MdixBlobInfo.SuggestedFileName("simpleTable.kik", MdixBlobKind.Png), "file name from dotted path");
        T.Eq("icon.png", MdixBlobInfo.SuggestedFileName("items[2].icon", MdixBlobKind.Png), "file name drops [n] in the middle");
        T.Eq("arr.bin", MdixBlobInfo.SuggestedFileName("arr[0]", MdixBlobKind.Unknown), "file name drops trailing [n]");
        T.Eq("arr.wav", MdixBlobInfo.SuggestedFileName("arr[0][1]", MdixBlobKind.Wav), "file name drops several trailing [n]");
        T.Eq("blob.bin", MdixBlobInfo.SuggestedFileName("", MdixBlobKind.Unknown), "file name from nothing"); T.Eq("blob.txt", MdixBlobInfo.SuggestedFileName(null, MdixBlobKind.Text), "file name from null");
        T.Eq("a_b_c.png", MdixBlobInfo.SuggestedFileName("a b/c", MdixBlobKind.Png), "file name made safe");
        T.Eq("blob.png", MdixBlobInfo.SuggestedFileName("..:::", MdixBlobKind.Png), "file name with nothing usable");
        T.True(!MdixBlobInfo.SuggestedFileName("x\\y*?:<>|\"", MdixBlobKind.Png).Any(c => "\\/*?:<>|\"".Contains(c)), "no path or invalid file characters survive");

        int fw, fh;
        MdixBlobInfo.FitSize(800, 600, 400, 240, out fw, out fh); T.True(fw == 320 && fh == 240, "fit wide image: " + fw + "x" + fh);
        MdixBlobInfo.FitSize(100, 50, 400, 240, out fw, out fh);  T.True(fw == 100 && fh == 50, "never scaled up");
        MdixBlobInfo.FitSize(0, 0, 400, 240, out fw, out fh);     T.True(fw == 400 && fh == 240, "unknown size falls back to the box");
        MdixBlobInfo.FitSize(1, 1000, 400, 240, out fw, out fh);  T.True(fw == 1 && fh == 240, "thin image keeps at least 1px");
        MdixBlobInfo.FitSize(4000, 100, 400, 240, out fw, out fh); T.True(fw == 400 && fh == 10, "wide strip");

        // hex dump
        var hx = new byte[] { 0x89,0x50,0x4E,0x47,0x0D,0x0A,0x1A,0x0A, 0,0,0,0x0D,0x49,0x48,0x44,0x52, 0,0,1,0 };
        var dump = MdixBlobInfo.HexDump(hx, hx.Length); var dl = dump.Split('\n');
        T.Eq(2, dl.Length, "hex dump: two lines");
        T.Eq("00000000  89 50 4E 47 0D 0A 1A 0A  00 00 00 0D 49 48 44 52  |.PNG........IHDR|", dl[0], "hex dump: first line exact");
        T.True(dl[1].StartsWith("00000010  00 00 01 00 "), "hex dump: second line offset and bytes");
        T.Eq(dl[0].IndexOf(" |"), dl[1].IndexOf(" |"), "hex dump: a short last line still lines up its ascii column");
        T.True(dl[1].EndsWith("|...."+"|") || dl[1].EndsWith("|....|"), "hex dump: ascii of short line");
        T.Eq("", MdixBlobInfo.HexDump(new byte[0], 0), "hex dump: empty");
        var big = new byte[300]; for (int i = 0; i < big.Length; i++) big[i] = (byte)i;
        var bigDump = MdixBlobInfo.HexDump(big, 1000).Split('\n');
        T.Eq(17, bigDump.Length, "hex dump: 16 lines + the more-bytes line"); T.Eq("… 744 more bytes", bigDump[16], "hex dump: counts what is not shown");
        T.True(!MdixBlobInfo.HexDump(big.Take(32).ToArray(), 32).Contains("more bytes"), "hex dump: no more-bytes line when all shown");
        T.True(bigDump[15].StartsWith("000000F0  F0 F1"), "hex dump: last shown offset");

        // text preview
        bool trunc;
        T.Eq("hello\nworld.\t!", MdixBlobInfo.TextPreview(Encoding.UTF8.GetBytes("hello\r\nworld\0\t!"), 15, out trunc), "text preview: line endings, control chars"); T.True(!trunc, "text preview: complete");
        T.Eq("a\nb", MdixBlobInfo.TextPreview(Encoding.UTF8.GetBytes("a\rb"), 3, out trunc), "text preview: lone CR");
        var longText = new string('x', 10000); var longHead = Encoding.UTF8.GetBytes(longText).Take(MdixBlobInfo.TextPreviewBytes).ToArray();
        var lp = MdixBlobInfo.TextPreview(longHead, 10000, out trunc); T.True(lp.Length == MdixBlobInfo.TextPreviewChars && trunc, "text preview: cut at 4000 chars and flagged");
        MdixBlobInfo.TextPreview(Encoding.UTF8.GetBytes("short"), 9999, out trunc); T.True(trunc, "text preview: flagged when the blob is longer than what was read");
        T.Eq("é世😀", MdixBlobInfo.TextPreview(Encoding.UTF8.GetBytes("é世😀"), 9, out trunc), "text preview: UTF-8");
        T.Eq("", MdixBlobInfo.TextPreview(null, 0, out trunc), "text preview: nothing");

        T.True(MdixBlobInfo.MaxReplaceBytes == 1048576 && MdixBlobInfo.MaxPreviewBytes == 8388608, "limits as documented");

        // temp files
        var oldRoot = MdixBlobTemp.Root; var tmpRoot = Path.Combine(Path.GetTempPath(), "mdix-harness-" + Guid.NewGuid().ToString("N").Substring(0, 6));
        MdixBlobTemp.Root = tmpRoot;
        try
        {
            var tf = MdixBlobTemp.Write(new byte[] { 1, 2, 3 }, "audio", ".wav");
            T.True(File.Exists(tf) && tf.StartsWith(tmpRoot) && tf.EndsWith(".wav") && Path.GetFileName(tf).StartsWith("audio-"), "temp: written in the root with stem and extension");
            T.True(File.ReadAllBytes(tf).SequenceEqual(new byte[] { 1, 2, 3 }), "temp: content");
            var evil = MdixBlobTemp.Write(new byte[0], "../../evil", ".bin");
            T.Eq(tmpRoot, Path.GetDirectoryName(evil), "temp: a path-like stem cannot leave the root"); T.True(Path.GetFileName(evil).StartsWith("______evil-"), "temp: stem made safe (" + Path.GetFileName(evil) + ")");
            var blank = MdixBlobTemp.Write(null, "", null); T.True(Path.GetFileName(blank).StartsWith("blob-") && File.ReadAllBytes(blank).Length == 0, "temp: blank stem, null data");
            MdixBlobTemp.Delete(tf); T.True(!File.Exists(tf), "temp: deleted"); MdixBlobTemp.Delete(tf); MdixBlobTemp.Delete(null); MdixBlobTemp.Delete("");   // none of these throw
            File.SetLastWriteTimeUtc(evil, DateTime.UtcNow - TimeSpan.FromDays(2));
            MdixBlobTemp.CleanOld(TimeSpan.FromDays(1));
            T.True(!File.Exists(evil) && File.Exists(blank), "temp: cleanup removes only old files");
            MdixBlobTemp.Root = Path.Combine(tmpRoot, "nope"); MdixBlobTemp.CleanOld(TimeSpan.Zero);   // missing folder is fine
        }
        finally { MdixBlobTemp.Root = oldRoot; try { Directory.Delete(tmpRoot, true); } catch { } }

        // ── J. Integers, colours, enums ──────────────────────────────────
        bool PI(string t, out long v, out bool l, out int r) => MdixIntegerText.TryParse(t, out v, out l, out r);
        void Int(string t, long want, bool wantLong, int wantRadix)
        {
            T.True(PI(t, out var v, out var l, out var r), "int parses: " + t);
            T.True(v == want && l == wantLong && r == wantRadix, "int " + t + " -> " + v + (l ? "L" : "") + " radix " + r + " (want " + want + (wantLong ? "L" : "") + " radix " + wantRadix + ")");
        }
        Int("255", 255, false, 10); Int("-5", -5, false, 10); Int("1_000_000", 1000000, false, 10); Int("3_000_000_000", 3000000000, true, 10); Int("5L", 5, true, 10);
        Int("0xFF", 255, false, 16); Int("0XFF", 255, false, 16); Int("0xFF_FF", 65535, false, 16); Int("0x7FFFFFFF", 2147483647, false, 16);
        Int("0x80000000", 2147483648, true, 16); Int("0xFFFFFFFF", 4294967295, true, 16); Int("0xDEAD_BEEF", 3735928559, true, 16); Int("0xFFL", 255, true, 16); Int("0xffl", 255, true, 16);
        Int("0b1010", 10, false, 2); Int("0B101", 5, false, 2); Int("0b1111_0000", 240, false, 2); Int("0b1L", 1, true, 2); Int("0b0", 0, false, 2);
        Int("9223372036854775807", long.MaxValue, true, 10); Int("0x7FFFFFFFFFFFFFFF", long.MaxValue, true, 16); Int(" 0xFF ", 255, false, 16);
        Int("0b" + new string('1', 31), 2147483647, false, 2); Int("0b1" + new string('0', 31), 2147483648, true, 2); Int("0b" + new string('1', 63), long.MaxValue, true, 2);
        foreach (var badInt in new[] { "", " ", "x", "0x", "0xG", "0b", "0b102", "0b2", "-0xFF", "-0b1", "0x8000000000000000", "9223372036854775808", "0b" + new string('1', 64), "1.5", "1e3", "0xFF.5", "_5", "5_", "0xL" })
            T.True(!PI(badInt, out _, out _, out _), "int refused: '" + badInt + "'");
        T.Eq(16, MdixIntegerText.RadixOf("0xFF"), "radix hex"); T.Eq(2, MdixIntegerText.RadixOf("0B1"), "radix bin"); T.Eq(10, MdixIntegerText.RadixOf("0"), "radix of 0"); T.Eq(10, MdixIntegerText.RadixOf("-0x1"), "radix of -0x1 is not hex"); T.Eq(10, MdixIntegerText.RadixOf(null), "radix of null");
        T.Eq("0xFF", MdixIntegerText.Format(255, 16, false), "format hex"); T.Eq("0b1010L", MdixIntegerText.Format(10, 2, true), "format bin long"); T.Eq("-7", MdixIntegerText.Format(-7, 10, false), "format dec");

        // rewriting keeps notation and type
        string rwl; long rwv; string rwe;
        T.True(MdixIntegerText.TryRewrite("0xFF", false, "300", out rwl, out rwv, out rwe) && rwl == "0x12C" && rwv == 300, "hex int rewritten from decimal input: " + rwl);
        T.True(MdixIntegerText.TryRewrite("0xFF", false, "0b101", out rwl, out rwv, out rwe) && rwl == "0x5", "hex int rewritten from binary input");
        T.True(MdixIntegerText.TryRewrite("0b1010", false, "0xFF", out rwl, out rwv, out rwe) && rwl == "0b11111111", "binary int rewritten from hex input: " + rwl);
        T.True(MdixIntegerText.TryRewrite("12", false, "0xFF", out rwl, out rwv, out rwe) && rwl == "255", "decimal int rewritten from hex input");
        T.True(!MdixIntegerText.TryRewrite("0xFF", false, "0x80000000", out rwl, out rwv, out rwe) && rwe.Contains("32-bit"), "hex int refuses a value that would become a long");
        T.True(!MdixIntegerText.TryRewrite("0xFF", false, "-1", out rwl, out rwv, out rwe) && rwe.Contains("negative"), "hex int refuses a negative");
        T.True(!MdixIntegerText.TryRewrite("5", false, "3000000000", out rwl, out rwv, out rwe), "decimal int refuses a value that would become a long");
        T.True(MdixIntegerText.TryRewrite("-5", false, "-9", out rwl, out rwv, out rwe) && rwl == "-9", "decimal negative kept");
        T.True(MdixIntegerText.TryRewrite("0xFFL", true, "16", out rwl, out rwv, out rwe) && rwl == "0x10L", "long hex keeps its L");
        T.True(MdixIntegerText.TryRewrite("0xDEAD_BEEF", true, "0xFFFFFFFF", out rwl, out rwv, out rwe) && rwl == "0xFFFFFFFF", "promoted long hex needs no L while it is beyond int");
        T.True(MdixIntegerText.TryRewrite("0xDEAD_BEEF", true, "0x10", out rwl, out rwv, out rwe) && rwl == "0x10L", "promoted long hex gets an L once the value fits an int");
        T.True(MdixIntegerText.TryRewrite("5L", true, "7", out rwl, out rwv, out rwe) && rwl == "7L", "decimal long keeps L");
        T.True(MdixIntegerText.TryRewrite("3000000000", true, "4000000000", out rwl, out rwv, out rwe) && rwl == "4000000000", "promoted decimal long: no L needed");
        T.True(!MdixIntegerText.TryRewrite("5L", true, "abc", out rwl, out rwv, out rwe) && rwe == "Enter a whole number.", "long refuses text");
        T.True(MdixLiteralEdit.TryFormat(MdixLiteralKind.Int, "0xFF", "0x1F", out var il, out var ie, out var ier) && il == "0x1F" && ie == "31", "TryFormat int hex: " + il + " expected " + ie);
        T.True(MdixLiteralEdit.TryFormat(MdixLiteralKind.Long, "9L", "10", out il, out ie, out ier) && il == "10L" && ie == "10", "TryFormat long");

        // colours: 4 digits are #RGBA
        float cr, cg, cb, ca;
        T.True(MdixSourceMap.TryParseHex("#F80C", out cr, out cg, out cb, out ca) && Math.Abs(cr - 1f) < 1e-4 && Math.Abs(cg - 0x88/255f) < 1e-4 && Math.Abs(cb) < 1e-4 && Math.Abs(ca - 0xCC/255f) < 1e-4, "#F80C = FF8800CC");
        T.True(MdixSourceMap.TryParseHex("#F80", out cr, out cg, out cb, out ca) && ca == 1f, "#F80 opaque");
        foreach (var badc in new[] { "#", "#F", "#F8", "#F80C0", "#F80C0DE", "#GGG", "#12345" , "F80" })
            T.True(!MdixSourceMap.TryParseHex(badc, out _, out _, out _, out _), "colour refused: " + badc);
        T.True(MdixLiteralEdit.TryFormat(MdixLiteralKind.HexColor, "#F80", "#f80c", out il, out ie, out ier) && il == "#F80C", "TryFormat 4-digit colour keeps its length");

        // spans in a source
        var c5 = @"@ENUMS(
  Difficulty { EASY, NORMAL, HARD }
  Rarity { COMMON = 1, RARE = 2, EPIC = 3, DEBUFF = -1 }
)
@DATA(
  h1 = 0xFF
  h2 = 0xDEAD_BEEF
  h3 = 0xFFL
  b1 = 0b1010
  b2 = 0B1111_0000L
  big = 0xFFFFFFFF
  bad1 = 0x
  bad2 = 0xZZ
  c4 = #F80C
  c3 = #F80
  c5 = #F80C0
  d = Difficulty.NORMAL
  r = Rarity.DEBUFF
  nope = Difficulty.EXTREME
  other = Missing.NORMAL
  spaced = Difficulty . HARD
  expr = Difficulty.NORMAL + 1
  arr = [Difficulty.EASY, Difficulty.HARD]
  obj = { level = Rarity.EPIC, mask = 0xF0 }
  tup = t:(Difficulty.NORMAL, 0xFF)
)";
        T.Eq("Int:0xFF", Lit(c5, "h1"), "hex int span"); T.Eq("Long:0xDEAD_BEEF", Lit(c5, "h2"), "hex long (promoted) span"); T.Eq("Long:0xFFL", Lit(c5, "h3"), "hex long with L");
        T.Eq("Int:0b1010", Lit(c5, "b1"), "binary int span"); T.Eq("Long:0B1111_0000L", Lit(c5, "b2"), "binary long span"); T.Eq("Long:0xFFFFFFFF", Lit(c5, "big"), "0xFFFFFFFF is a long");
        T.Eq("(unmapped)", Lit(c5, "bad1"), "empty hex unmapped"); T.Eq("(unmapped)", Lit(c5, "bad2"), "bad hex unmapped");
        T.Eq("HexColor:#F80C", Lit(c5, "c4"), "4-digit colour mapped"); T.Eq("HexColor:#F80", Lit(c5, "c3"), "3-digit colour"); T.Eq("(unmapped)", Lit(c5, "c5"), "5-digit colour unmapped");
        T.Eq("Enum:Difficulty.NORMAL", Lit(c5, "d"), "enum value mapped"); T.Eq("Enum:Rarity.DEBUFF", Lit(c5, "r"), "enum with negative member values");
        T.Eq("(unmapped)", Lit(c5, "nope"), "unknown member unmapped"); T.Eq("(unmapped)", Lit(c5, "other"), "undeclared enum unmapped");
        T.Eq("(unmapped)", Lit(c5, "spaced"), "spaced dot unmapped"); T.Eq("(unmapped)", Lit(c5, "expr"), "enum in an expression unmapped");
        T.Eq("Enum:Difficulty.EASY", Lit(c5, "arr[0]"), "enum in array 0"); T.Eq("Enum:Difficulty.HARD", Lit(c5, "arr[1]"), "enum in array 1");
        T.Eq("Enum:Rarity.EPIC", Lit(c5, "obj.level"), "enum in object"); T.Eq("Int:0xF0", Lit(c5, "obj.mask"), "hex in object");
        T.Eq("(unmapped)", Lit(c5, "tup"), "tuple still unmapped");

        var en = MdixSourceMap.BuildEnums(c5);
        T.Eq(2, en.Count, "two enums read"); T.True(en["Difficulty"].Members.Select(m => m.Name).SequenceEqual(new[] { "EASY", "NORMAL", "HARD" }), "members in order");
        T.True(en["Difficulty"].Members.All(m => m.Value == null), "no explicit values"); T.True(en["Rarity"].Members.Select(m => m.Value).SequenceEqual(new[] { "1", "2", "3", "-1" }), "explicit values kept, negative included");
        T.True(en["Rarity"].Has("EPIC") && !en["Rarity"].Has("epic") && !en["Rarity"].Has("NOPE"), "Has is exact");
        T.Eq(0, MdixSourceMap.BuildEnums("").Count, "no enums in nothing"); T.Eq(0, MdixSourceMap.BuildEnums("@DATA( a = 1 )").Count, "no enums section");
        T.Eq(1, MdixSourceMap.BuildEnums("@Enums(\n A { X, Y }\n)").Count, "section name is case-insensitive");
        T.Eq(0, MdixSourceMap.BuildEnums("@ENUMS( A { X, Y ").Count == 0 ? 0 : 0, "unterminated enums do not throw");

        // enum TryFormat
        T.True(MdixLiteralEdit.TryFormat(MdixLiteralKind.Enum, "Difficulty.NORMAL", "HARD", out il, out ie, out ier) && il == "Difficulty.HARD" && ie == il, "enum: bare member");
        T.True(MdixLiteralEdit.TryFormat(MdixLiteralKind.Enum, "Difficulty.NORMAL", "Difficulty.EASY", out il, out ie, out ier) && il == "Difficulty.EASY", "enum: qualified member");
        T.True(!MdixLiteralEdit.TryFormat(MdixLiteralKind.Enum, "Difficulty.NORMAL", "Rarity.EPIC", out il, out ie, out ier) && ier.Contains("Difficulty"), "enum: another enum's member refused");
        T.True(!MdixLiteralEdit.TryFormat(MdixLiteralKind.Enum, "Difficulty.NORMAL", "not valid!", out il, out ie, out ier), "enum: bad name refused");
        T.True(!MdixLiteralEdit.TryFormat(MdixLiteralKind.Enum, "Difficulty.NORMAL", "", out il, out ie, out ier), "enum: empty refused");

        // differential corpus for the real compiler (read by the python check)
        var corpus = new List<string> { "0", "1", "-1", "255", "2147483647", "2147483648", "-2147483648", "-2147483649", "5L", "0L", "1_000", "3_000_000_000", "9223372036854775807",
            "0x0", "0xF", "0xFF", "0XFF", "0xff", "0xFF_FF", "0x7FFFFFFF", "0x80000000", "0xFFFFFFFF", "0xDEAD_BEEF", "0xFFL", "0xDEADBEEFL", "0x7FFFFFFFFFFFFFFF", "0xFFFFFFFFFFFFFFFF",
            "0b0", "0b1", "0b1010", "0B101", "0b1111_0000", "0b1L", "0b" + new string('1', 31), "0b1" + new string('0', 31), "0b" + new string('1', 63), "0b" + new string('1', 64),
            "0x", "0xG", "0b", "0b2", "0b102" };
        File.WriteAllLines("/tmp/int_corpus.tsv", corpus.Select(t => { var ok = PI(t, out var v, out var l, out var r); return t + "\t" + (ok ? "ok" : "fail") + "\t" + v + "\t" + (l ? "Long" : "Int"); }));
        File.WriteAllLines("/tmp/color_corpus.tsv", new[] { "#F", "#F8", "#F80", "#F80C", "#F80C0", "#FF8800", "#FF8800C", "#FF8800CC", "#FF8800CC1", "#GGG", "#12345", "#ABCD", "#abcd" }
            .Select(t => { var ok = MdixSourceMap.TryParseHex(t, out var rr, out var gg, out var bb, out var aa); return t + "\t" + (ok ? "ok" : "fail") + "\t" + rr.ToString("0.####", System.Globalization.CultureInfo.InvariantCulture) + "\t" + gg.ToString("0.####", System.Globalization.CultureInfo.InvariantCulture) + "\t" + bb.ToString("0.####", System.Globalization.CultureInfo.InvariantCulture) + "\t" + aa.ToString("0.####", System.Globalization.CultureInfo.InvariantCulture); }));

        // group arrays ("name:: 1, 2, 3") map their plain rows; function-call rows stay unmapped
        T.Eq("Int:3", Lit(inv, "testArray[2]"), "real file: group array item mapped"); T.Eq("Int:1", Lit(inv, "testArray[0]"), "real file: first group array item");
        T.Eq("(unmapped)", Lit(inv, "items[0]"), "real file: function-call row stays unmapped");
        var ga = @"@DATA(
  names:: ""a"", ""b"", Rarity.X
  rows:: { x = 1 }, { x = 2 }
  calls:: make(1, 2), make(3, 4)
)";
        T.Eq("String:\"a\"", Lit(ga, "names[0]"), "group array string row"); T.Eq("String:\"b\"", Lit(ga, "names[1]"), "group array second row");
        T.Eq("Int:1", Lit(ga, "rows[0].x"), "object rows still map their members"); T.Eq("Int:2", Lit(ga, "rows[1].x"), "second object row");
        T.Eq("(unmapped)", Lit(ga, "calls[0]"), "call rows unmapped"); T.Eq("(unmapped)", Lit(ga, "calls[1]"), "second call row unmapped");

        // ── K. Sample media and sample files ─────────────────────────────
        if (Directory.Exists("/tmp/media"))
        {
            var want = new Dictionary<string, MdixBlobKind> {
                { "icon.png", MdixBlobKind.Png }, { "icon.jpg", MdixBlobKind.Jpeg }, { "icon.gif", MdixBlobKind.Gif }, { "icon.bmp", MdixBlobKind.Bmp },
                { "icon.webp", MdixBlobKind.Webp }, { "tone.wav", MdixBlobKind.Wav }, { "tone.ogg", MdixBlobKind.Ogg }, { "tone.mp3", MdixBlobKind.Mp3 },
                { "note.txt", MdixBlobKind.Text }, { "noise.bin", MdixBlobKind.Unknown } };
            foreach (var kv in want)
            {
                var bytes = File.ReadAllBytes("/tmp/media/" + kv.Key);
                var head = MdixBlobInfo.HeadBytes(Convert.ToBase64String(bytes), MdixBlobInfo.SniffBytes);
                T.Eq(kv.Value, MdixBlobInfo.Sniff(head, head.Length), "sample media sniffed: " + kv.Key);
                T.Eq((long)bytes.Length, MdixBlobInfo.DecodedLength(Convert.ToBase64String(bytes)), "sample media length: " + kv.Key);
            }
        }
        if (Directory.Exists("/home/claude/w/samples_out"))
        {
            foreach (var f in Directory.GetFiles("/home/claude/w/samples_out", "*.mdix"))
            {
                var src = File.ReadAllText(f);
                var map = MdixSourceMap.Build(src);
                File.WriteAllLines("/tmp/spans_" + Path.GetFileName(f) + ".tsv",
                    map.OrderBy(k => k.Value.Start).Select(k => k.Key + "\t" + k.Value.Kind + "\t" + Convert.ToBase64String(Encoding.UTF8.GetBytes(k.Value.TextIn(src)))));
                T.True(map.Count > 10, "sample " + Path.GetFileName(f) + " maps " + map.Count + " literals");
            }
        }

        // ── L. The embedded samples are the validated files, byte for byte ─
        if (Directory.Exists("/home/claude/w/samples_out"))
        {
            T.Eq(File.ReadAllText("/home/claude/w/samples_out/ExplorerTour.mdix"), MdixSampleData.Tour, "embedded ExplorerTour == the validated file");
            T.Eq(File.ReadAllText("/home/claude/w/samples_out/NumbersEnumsAndLists.mdix"), MdixSampleData.Numbers, "embedded NumbersEnumsAndLists == the validated file");
            T.Eq(File.ReadAllText("/home/claude/w/samples_out/README.md"), MdixSampleData.Readme, "embedded README == the generated file");
            T.True(!MdixSampleData.Tour.Contains("\r") && !MdixSampleData.Numbers.Contains("\r") && !MdixSampleData.Readme.Contains("\r"), "samples hold only \\n line endings as embedded");
            T.True(MdixSampleData.Tour.Contains("raw_notes<string> = `Raw string: nothing is escaped.\nBackslashes stay as they are: C:\\Games"), "raw string kept its backslashes and line breaks");
            T.True(MdixSampleData.Tour.Contains("r:(\"^\\\\d+$\")"), "regex escapes survived the embedding");
        }

        // ── M. Tuples and colours ────────────────────────────────────────
        MidManStudio.Mdix.Unity.Editor.Lsp.MdixJsonValue J(string json) => MidManStudio.Mdix.Unity.Editor.Lsp.MdixJsonValue.Parse(json);
        string tp; int[] tix;
        var tupleSet = new HashSet<string> { "t", "a.b", "arr[2]", "grid" };
        bool Split(string path) => MdixTupleText.TrySplitPath(path, x => tupleSet.Contains(x), out tp, out tix);
        T.True(Split("t[1]") && tp == "t" && tix.SequenceEqual(new[] { 1 }), "split: t[1]");
        T.True(Split("t[1][0][3]") && tp == "t" && tix.SequenceEqual(new[] { 1, 0, 3 }), "split: nested indexes in order");
        T.True(Split("a.b[0]") && tp == "a.b" && tix.SequenceEqual(new[] { 0 }), "split: tuple in an object");
        T.True(Split("arr[2][1]") && tp == "arr[2]" && tix.SequenceEqual(new[] { 1 }), "split: tuple that is an array item");
        T.True(!Split("t"), "split: the tuple itself is not an element"); T.True(!Split("arr[2]"), "split: an array item that is a tuple is not an element");
        T.True(!Split("other[0]"), "split: not inside any tuple"); T.True(!Split("x.y"), "split: no index"); T.True(!Split(""), "split: empty"); T.True(!Split("[0]"), "split: bare index"); T.True(!Split("t[x]"), "split: non-numeric index");
        T.True(!Split("t[-1]"), "split: negative index");

        var tj = J("[1,[2,[3,\"x\"]],true]");
        T.Eq(1.0, MdixTupleText.Navigate(tj, new[] { 0 }).AsDouble(), "navigate: first"); T.Eq(3.0, MdixTupleText.Navigate(tj, new[] { 1, 1, 0 }).AsDouble(), "navigate: deep");
        T.True(MdixTupleText.Navigate(tj, new[] { 5 }) == null && MdixTupleText.Navigate(tj, new[] { 0, 0 }) == null && MdixTupleText.Navigate(tj, new[] { -1 }) == null, "navigate: out of range or into a scalar is null");
        T.True(MdixTupleText.SameExcept(J("[1,2,3]"), J("[1,9,3]"), new[] { 1 }), "same-except: the element may change");
        T.True(!MdixTupleText.SameExcept(J("[1,2,3]"), J("[1,2,4]"), new[] { 1 }), "same-except: another element changing is caught");
        T.True(!MdixTupleText.SameExcept(J("[1,2,3]"), J("[1,2]"), new[] { 1 }), "same-except: a different length is caught");
        T.True(MdixTupleText.SameExcept(J("[1,[2,3]]"), J("[1,[2,8]]"), new[] { 1, 1 }), "same-except: nested element");
        T.True(!MdixTupleText.SameExcept(J("[1,[2,3]]"), J("[1,[9,8]]"), new[] { 1, 1 }), "same-except: nested sibling change caught");
        T.True(!MdixTupleText.SameExcept(J("[1,2]"), J("{\"a\":1}"), new[] { 0 }), "same-except: shape change caught");

        // element matching, with the JSON shapes the real engine produces (captured above)
        bool EM(MdixLiteralKind k, string lit, string json) => MdixTupleText.ElementMatches(k, lit, J(json));
        T.True(EM(MdixLiteralKind.Int, "255", "255") && EM(MdixLiteralKind.Int, "0xFF", "255") && EM(MdixLiteralKind.Int, "0b101", "5") && EM(MdixLiteralKind.Int, "-2", "-2"), "element: ints in any notation");
        T.True(!EM(MdixLiteralKind.Int, "5", "6") && !EM(MdixLiteralKind.Int, "5", "\"5\"") && !EM(MdixLiteralKind.Int, "5L", "5"), "element: wrong int refused (and an L literal is not an Int)");
        T.True(EM(MdixLiteralKind.Long, "3_000_000_000", "3000000000") && EM(MdixLiteralKind.Long, "0xFFFFFFFF", "4294967295") && EM(MdixLiteralKind.Long, "5L", "5"), "element: longs");
        T.True(EM(MdixLiteralKind.Float, "1.5f", "1.5") && EM(MdixLiteralKind.Double, "2.25", "2.25") && EM(MdixLiteralKind.Double, "-0.5e2", "-50.0") && EM(MdixLiteralKind.Double, "3.0", "3.0"), "element: floats and doubles");
        T.True(!EM(MdixLiteralKind.Double, "2.25", "2.5"), "element: wrong double refused");
        T.True(EM(MdixLiteralKind.Bool, "true", "true") && EM(MdixLiteralKind.Bool, "false", "false") && !EM(MdixLiteralKind.Bool, "true", "false"), "element: bools");
        T.True(EM(MdixLiteralKind.String, "\"a\\nb\"", "\"a\\nb\"") && EM(MdixLiteralKind.String, "'q'", "\"q\"") && EM(MdixLiteralKind.String, "\"\"", "\"\"") && !EM(MdixLiteralKind.String, "\"a\"", "\"b\""), "element: strings decode their escapes");
        T.True(EM(MdixLiteralKind.RawString, "`raw\\x`", "\"raw\\\\x\""), "element: raw string");
        T.True(EM(MdixLiteralKind.Date, "2026-01-15", "\"2026-01-15\"") && EM(MdixLiteralKind.Timestamp, "2025-01-15T10:30:00.250+05:30", "\"2025-01-15T10:30:00.250+05:30\""), "element: date and timestamp");
        T.True(EM(MdixLiteralKind.HexColor, "#F80C", "\"#F80C\"") && !EM(MdixLiteralKind.HexColor, "#F80C", "\"#F80D\""), "element: colour");
        T.True(EM(MdixLiteralKind.Regex, "r:(\"^a+$\")", "\"^a+$\"") && EM(MdixLiteralKind.Blob, "b:(\"AAEC\")", "\"AAEC\""), "element: regex and blob");
        T.True(EM(MdixLiteralKind.Enum, "E.B", "{\"enum_name\":\"E\",\"field_name\":\"B\",\"value\":5}") && !EM(MdixLiteralKind.Enum, "E.A", "{\"enum_name\":\"E\",\"field_name\":\"B\",\"value\":5}"), "element: enum");
        T.True(!MdixTupleText.ElementMatches(MdixLiteralKind.Int, "5", null), "element: missing element refused");

        T.Eq("255", MdixTupleText.ElementEditText(MdixLiteralKind.Int, "255"), "edit text: int"); T.Eq("1000", MdixTupleText.ElementEditText(MdixLiteralKind.Int, "1_000"), "edit text: decimal int loses its separators");
        T.Eq("0xFF", MdixTupleText.ElementEditText(MdixLiteralKind.Int, "0xFF"), "edit text: hex notation kept"); T.Eq("0xFF", MdixTupleText.ElementEditText(MdixLiteralKind.Long, "0xFFL"), "edit text: long hex drops the L");
        T.Eq("5", MdixTupleText.ElementEditText(MdixLiteralKind.Long, "5L"), "edit text: decimal long"); T.Eq("1.5", MdixTupleText.ElementEditText(MdixLiteralKind.Float, "1.5f"), "edit text: float suffix dropped");
        T.Eq("a\nb", MdixTupleText.ElementEditText(MdixLiteralKind.String, "\"a\\nb\""), "edit text: string decoded"); T.Eq("raw\\x", MdixTupleText.ElementEditText(MdixLiteralKind.RawString, "`raw\\x`"), "edit text: raw");
        T.Eq("^a+$", MdixTupleText.ElementEditText(MdixLiteralKind.Regex, "r:(\"^a+$\")"), "edit text: regex"); T.Eq("E.B", MdixTupleText.ElementEditText(MdixLiteralKind.Enum, "E.B"), "edit text: enum");

        // showing a tuple
        T.True(MdixTupleText.SplitElements("[1,[2,3],\"a,b\",{\"x\":[1,2]}]").SequenceEqual(new[] { "1", "[2,3]", "\"a,b\"", "{\"x\":[1,2]}" }), "split: commas inside strings and nests stay together");
        T.True(MdixTupleText.SplitElements("[]").Count == 0 && MdixTupleText.SplitElements("[ ]").Count == 0, "split: empty tuple"); T.True(MdixTupleText.SplitElements("[1]").SequenceEqual(new[] { "1" }), "split: one element");
        T.True(MdixTupleText.SplitElements("[\"a\\\"b\",2]").SequenceEqual(new[] { "\"a\\\"b\"", "2" }), "split: escaped quote inside a string"); T.True(MdixTupleText.SplitElements("5").Count == 0 && MdixTupleText.SplitElements(null).Count == 0, "split: not an array");
        T.Eq("(1.5, 2.5, 0.0)", MdixTupleText.Summary("[1.5,2.5,0.0]"), "summary: numbers keep their text"); T.Eq("(1, (2, (3, \"x\")))", MdixTupleText.Summary("[1,[2,[3,\"x\"]]]"), "summary: nested");
        T.Eq("(\"Sword\", 3, true, 2.5, E.B)", MdixTupleText.Summary("[\"Sword\",3,true,2.5,{\"enum_name\":\"E\",\"field_name\":\"B\",\"value\":5}]"), "summary: enum shown as Type.MEMBER");
        T.Eq("()", MdixTupleText.Summary("[]"), "summary: empty"); T.Eq("({…})", MdixTupleText.Summary("[{\"a\":1}]"), "summary: other object");
        T.True(MdixTupleText.Summary("[" + string.Join(",", Enumerable.Range(0, 100)) + "]", 30).Length == 30 && MdixTupleText.Summary("[" + string.Join(",", Enumerable.Range(0, 100)) + "]", 30).EndsWith("…"), "summary: cut with an ellipsis");
        T.Eq("int", MdixTupleText.Chip("5"), "chip int"); T.Eq("float", MdixTupleText.Chip("5.0"), "chip float"); T.Eq("float", MdixTupleText.Chip("1e3"), "chip exponent"); T.Eq("string", MdixTupleText.Chip("\"x\""), "chip string");
        T.Eq("bool", MdixTupleText.Chip("true"), "chip bool"); T.Eq("tuple", MdixTupleText.Chip("[1]"), "chip tuple"); T.Eq("enum", MdixTupleText.Chip("{\"enum_name\":\"E\",\"field_name\":\"A\",\"value\":0}"), "chip enum");
        T.Eq("object", MdixTupleText.Chip("{\"a\":1}"), "chip object"); T.Eq("null", MdixTupleText.Chip("null"), "chip null"); T.True(MdixTupleText.IsNested(" [1]") && !MdixTupleText.IsNested("1") && !MdixTupleText.IsNested(null), "is-nested");

        // source map: tuple elements
        var tt = @"@ENUMS(
  E { A, B }
)
@DATA(
  t1 = t:(1.5, ""x"", true, E.B, #F80C, 0xFF)
  t2 = t:(1, t:(2, t:(3, ""deep"")))
  t3 = t:(f(1, 2), 3, 4 + 5, 6)
  t4 = t:([1, 2], ""z"")
  t5 = t:()
  t6 = t:(1,2).x
  t7 = t:(1,
          2,
          3)
  t8 = t:(2026-01-15, 2025-01-15T10:30:00Z, r:(""^a""), `raw`)
  obj = { inner = t:(7, 8) }
  arr = [t:(1, 2), t:(3, 4)]
  matrix = t:(t:(01,02,03),t:(04,05,06))
  rows:: t:(1, 2), t:(3, 4)
  plain = 5
)";
        T.Eq("Double:1.5", Lit(tt, "t1[0]"), "tuple: double element"); T.Eq("String:\"x\"", Lit(tt, "t1[1]"), "tuple: string"); T.Eq("Bool:true", Lit(tt, "t1[2]"), "tuple: bool");
        T.Eq("Enum:E.B", Lit(tt, "t1[3]"), "tuple: enum"); T.Eq("HexColor:#F80C", Lit(tt, "t1[4]"), "tuple: 4-digit colour"); T.Eq("Int:0xFF", Lit(tt, "t1[5]"), "tuple: hex int");
        T.Eq("(unmapped)", Lit(tt, "t1"), "tuple: the tuple itself is not a literal");
        T.Eq("Int:1", Lit(tt, "t2[0]"), "tuple: nested first"); T.Eq("Int:2", Lit(tt, "t2[1][0]"), "tuple: nested 2"); T.Eq("Int:3", Lit(tt, "t2[1][1][0]"), "tuple: nested 3"); T.Eq("String:\"deep\"", Lit(tt, "t2[1][1][1]"), "tuple: nested deepest");
        T.Eq("(unmapped)", Lit(tt, "t3[0]"), "tuple: function call element unmapped"); T.Eq("Int:3", Lit(tt, "t3[1]"), "tuple: element after a call keeps its index");
        T.Eq("(unmapped)", Lit(tt, "t3[2]"), "tuple: expression element unmapped"); T.Eq("Int:6", Lit(tt, "t3[3]"), "tuple: last element after an expression");
        T.Eq("Int:1", Lit(tt, "t4[0][0]"), "tuple: array inside a tuple"); T.Eq("Int:2", Lit(tt, "t4[0][1]"), "tuple: array item 1"); T.Eq("String:\"z\"", Lit(tt, "t4[1]"), "tuple: element after an array");
        T.Eq("(unmapped)", Lit(tt, "t5[0]"), "tuple: empty has no elements");
        T.Eq("(unmapped)", Lit(tt, "t6[0]"), "tuple: member access on a tuple is not mapped"); T.Eq("(unmapped)", Lit(tt, "t6[1]"), "tuple: ... nor its second element");
        T.Eq("Int:1", Lit(tt, "t7[0]"), "tuple: multi-line first"); T.Eq("Int:3", Lit(tt, "t7[2]"), "tuple: multi-line last");
        T.Eq("Date:2026-01-15", Lit(tt, "t8[0]"), "tuple: date"); T.Eq("Timestamp:2025-01-15T10:30:00Z", Lit(tt, "t8[1]"), "tuple: timestamp"); T.Eq("Regex:r:(\"^a\")", Lit(tt, "t8[2]"), "tuple: regex"); T.Eq("RawString:`raw`", Lit(tt, "t8[3]"), "tuple: raw string");
        T.Eq("Int:7", Lit(tt, "obj.inner[0]"), "tuple in an object"); T.Eq("Int:4", Lit(tt, "arr[1][1]"), "tuple as an array item"); T.Eq("Int:05", Lit(tt, "matrix[1][1]"), "tuple of tuples with leading zeros (the literal keeps its zero)");
        T.Eq("Int:4", Lit(tt, "rows[1][1]"), "tuple as a group-array row"); T.Eq("Int:1", Lit(tt, "rows[0][0]"), "first tuple row");
        T.Eq("Regex:r:(\"b\")", Lit("@DATA(\n  pats:: r:(\"a\"), r:(\"b\")\n)", "pats[1]"), "regex as a group-array row");
        T.Eq("Blob:b:(\"AA==\")", Lit("@DATA(\n  blobs::\n    b:(\"AAEC\"),\n    b:(\"AA==\")\n)", "blobs[1]"), "blob rows on separate lines"); T.Eq("Int:5", Lit(tt, "plain"), "a value after the tuples still maps");
        T.Eq(0, MdixSourceMap.Build("@DATA(\n  v = t:(1, 2\n)").Count == 0 ? 0 : 0, "unterminated tuple does not throw");

        // colours
        byte cr2, cg2, cb2, ca2; bool ha, sh;
        T.True(MdixColorText.TryParse("#F80C", out cr2, out cg2, out cb2, out ca2, out ha, out sh) && cr2 == 0xFF && cg2 == 0x88 && cb2 == 0 && ca2 == 0xCC && ha && sh, "colour: #F80C parsed");
        T.True(MdixColorText.TryParse("#ff8800", out cr2, out cg2, out cb2, out ca2, out ha, out sh) && cr2 == 255 && cg2 == 0x88 && ca2 == 255 && !ha && !sh, "colour: long form, opaque");
        T.True(MdixColorText.TryParse(" #FF8800CC ", out cr2, out cg2, out cb2, out ca2, out ha, out sh) && ca2 == 0xCC && ha && !sh, "colour: 8 digits, spaces ignored");
        foreach (var badc2 in new[] { "", "#", "#F", "#F8", "#F80C0", "#F80C0DE", "F80", "#GGG", "#F8 0" }) T.True(!MdixColorText.TryParse(badc2, out _, out _, out _, out _, out _, out _), "colour refused: '" + badc2 + "'");
        T.Eq("#F80", MdixColorText.Format(0xFF, 0x88, 0, 255, false, true), "format: exact short form kept"); T.Eq("#F80C", MdixColorText.Format(0xFF, 0x88, 0, 0xCC, true, true), "format: short with alpha");
        T.Eq("#FF8801", MdixColorText.Format(0xFF, 0x88, 1, 255, false, true), "format: not representable short -> long"); T.Eq("#FF8800", MdixColorText.Format(0xFF, 0x88, 0, 255, false, false), "format: long when short not preferred");
        T.Eq("#F80C", MdixColorText.Format(0xFF, 0x88, 0, 0xCC, false, true), "format: a translucent colour always carries alpha"); T.Eq("#FF8800FF", MdixColorText.Format(0xFF, 0x88, 0, 255, true, false), "format: alpha kept when the literal had it");
        T.Eq("#FF8800CD", MdixColorText.Format(0xFF, 0x88, 0, 0xCD, true, true), "format: alpha not representable short -> long");
        var crng = new Random(99); int cok = 0;
        for (int n = 0; n < 5000; n++)
        {
            var cr0 = (byte)crng.Next(256); var cg0 = (byte)crng.Next(256); var cb0 = (byte)crng.Next(256); var ca0 = (byte)(crng.Next(3) == 0 ? 255 : crng.Next(256));
            foreach (var withA in new[] { false, true }) foreach (var prefShort in new[] { false, true })
            {
                var text = MdixColorText.Format(cr0, cg0, cb0, ca0, withA, prefShort);
                T.True(MdixColorText.TryParse(text, out var r2, out var g2, out var b2, out var a2, out var h2, out var s2) && r2 == cr0 && g2 == cg0 && b2 == cb0 && a2 == ca0, "colour round trip " + text);
                T.True(MdixSourceMap.TryParseHex(text, out _, out _, out _, out _), "colour literal accepted by the source map: " + text);
                cok++;
            }
        }
        Console.WriteLine("colour round trips: " + cok);

        // differential: the real engine's tuple JSON against every mapped element, in the samples and in the real files
        int tupElems = 0, tupBad = 0;
        if (Directory.Exists("/tmp/tuples"))
        {
            foreach (var tsv in Directory.GetFiles("/tmp/tuples", "*.tsv"))
            {
                var name = Path.GetFileNameWithoutExtension(tsv);
                string srcPath = null;
                if (name.StartsWith("S_")) srcPath = "/home/claude/w/samples_out/" + name.Substring(2) + ".mdix";
                else foreach (var line in File.ReadAllLines("/tmp/spans_real/index.txt")) { var parts = line.Split('\t'); if (parts[0] == name) srcPath = parts[1]; }
                if (srcPath == null) continue;

                var tuples = File.ReadAllLines(tsv).Select(l => l.Split('\t')).ToDictionary(a => a[0], a => a[1]);
                var source = File.ReadAllText(srcPath);
                foreach (var kv in MdixSourceMap.Build(source))
                {
                    if (!MdixTupleText.TrySplitPath(kv.Key, x => tuples.ContainsKey(x), out var tpath, out var tidx)) continue;
                    tupElems++;
                    var ok = MdixTupleText.ElementMatches(kv.Value.Kind, kv.Value.TextIn(source), MdixTupleText.Navigate(J(tuples[tpath]), tidx));
                    if (!ok) { tupBad++; Console.WriteLine("  TUPLE MISMATCH " + Path.GetFileName(srcPath) + " :: " + kv.Key + "  " + kv.Value.Kind + "  " + kv.Value.TextIn(source)); }
                }
            }
            Console.WriteLine("tuple elements compared with the engine's JSON: " + tupElems + ", mismatches " + tupBad);
            T.True(tupElems > 0, "tuple elements were compared"); T.Eq(0, tupBad, "tuple elements agree with the engine");
        }

        Console.WriteLine();
        Console.WriteLine("PASS " + T.Pass + "   FAIL " + T.Fail);
        Environment.Exit(T.Fail == 0 ? 0 : 1);
    }

    static string Trunc(string s) => (s.Length > 60 ? s.Substring(0, 60) + "…" : s).Replace("\n", "\\n");
}
