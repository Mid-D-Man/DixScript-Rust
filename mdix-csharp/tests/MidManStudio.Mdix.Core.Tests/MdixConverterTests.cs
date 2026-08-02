using System;
using FluentAssertions;
using MidManStudio.Mdix;
using MidManStudio.Mdix.Core;
using Xunit;
using Xunit.Abstractions;

namespace MidManStudio.Mdix.Core.Tests
{
    public class MdixConverterTests
    {
        private readonly ITestOutputHelper _out;

        public MdixConverterTests(ITestOutputHelper output) => _out = output;

        private MdixDatabase Load(string source)
        {
            _out.WriteLine($"Source: {source}");
            return Dix.LoadStr(source).OrThrow();
        }

        // ── ToMdix ────────────────────────────────────────────────────────────

        [Fact]
        public void ToMdix_SimpleDatabase_ContainsDataSection()
        {
            using var db = Load("@DATA( port = 9000, host = \"srv.local\" )");
            var result = MdixConverter.ToMdix(db);
            _out.WriteLine(result.UnwrapOr("FAILED"));

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().Contain("@DATA(");
            result.SuccessResult.Should().Contain("port");
            result.SuccessResult.Should().Contain("9000");
        }

        [Fact]
        public void ToMdix_MinifiedMode_ShorterThanDefault()
        {
            using var db = Load("@DATA( x = 1, y = 2, z = 3 )");
            var normal   = MdixConverter.ToMdix(db, MdixFormatMode.Default).OrThrow();
            var minified = MdixConverter.ToMdix(db, MdixFormatMode.Minified).OrThrow();

            _out.WriteLine($"Normal: {normal.Length}  Minified: {minified.Length}");
            minified.Length.Should().BeLessThan(normal.Length);
        }

        [Fact]
        public void ToMdix_NullDatabase_ReturnsError()
        {
            MdixConverter.ToMdix(null!).IsFailure.Should().BeTrue();
        }

        [Fact]
        public void DixFacade_ToMdix_Delegates()
        {
            using var db = Load("@DATA( val = 7 )");
            var result = Dix.ToMdix(db);
            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().Contain("val");
        }

        // ── ToJson ────────────────────────────────────────────────────────────

        [Fact]
        public void ToJson_SimpleValues_ProducesValidJson()
        {
            using var db = Load("@DATA( port = 8080, name = \"Test\", flag = true )");
            var result = MdixConverter.ToJson(db);
            _out.WriteLine(result.UnwrapOr("FAILED"));

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().Contain("8080");
            result.SuccessResult.Should().Contain("Test");
        }

        [Fact]
        public void ToJson_Indented_ContainsNewlines()
        {
            using var db = Load("@DATA( x = 1, y = 2 )");
            var result = MdixConverter.ToJson(db, indented: true);
            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().Contain("\n");
        }

        [Fact]
        public void ToJson_NotIndented_NoBlanksLines()
        {
            using var db = Load("@DATA( x = 1, y = 2 )");
            var result = MdixConverter.ToJson(db, indented: false);
            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Trim().Should().NotContain("\n");
        }

        [Fact]
        public void ToJson_NullDatabase_ReturnsError()
        {
            MdixConverter.ToJson(null!).IsFailure.Should().BeTrue();
        }

        [Fact]
        public void DixFacade_ToJson_Delegates()
        {
            using var db = Load("@DATA( count = 42 )");
            var result = Dix.ToJson(db);
            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().Contain("42");
        }

        // ── FromJson ──────────────────────────────────────────────────────────

        [Fact]
        public void FromJson_ValidObject_ProducesReadableDatabase()
        {
            const string json = "{\"port\": 8080, \"host\": \"localhost\", \"ssl\": true}";
            using var db = MdixConverter.FromJson(json).OrThrow();
            _out.WriteLine($"IsValid: {db.IsValid}  EntryCount: {db.EntryCount}");

            db.IsValid.Should().BeTrue();
            db.GetInt("port").OrThrow().Should().Be(8080);
            db.GetString("host").OrThrow().Should().Be("localhost");
            db.GetBool("ssl").OrThrow().Should().BeTrue();
        }

        [Fact]
        public void FromJson_NestedObject_AccessibleByDottedPath()
        {
            const string json = "{\"server\": {\"port\": 9000, \"host\": \"srv.local\"}}";
            using var db = MdixConverter.FromJson(json).OrThrow();
            _out.WriteLine($"EntryCount: {db.EntryCount}");

            db.GetInt("server.port").OrThrow().Should().Be(9000);
            db.GetString("server.host").OrThrow().Should().Be("srv.local");
        }

        [Fact]
        public void FromJson_InvalidJson_ReturnsError()
        {
            var result = MdixConverter.FromJson("not json at all");
            _out.WriteLine($"Error: {(result.IsFailure ? result.Error.Message : "none")}");
            result.IsFailure.Should().BeTrue();
        }

        [Fact]
        public void FromJson_ArrayTopLevel_ReturnsError()
        {
            var result = MdixConverter.FromJson("[1, 2, 3]");
            _out.WriteLine($"Error: {(result.IsFailure ? result.Error.Message : "none")}");
            result.IsFailure.Should().BeTrue();
        }

        [Fact]
        public void FromJson_EmptyString_ReturnsError()
        {
            MdixConverter.FromJson(string.Empty).IsFailure.Should().BeTrue();
        }

        [Fact]
        public void DixFacade_LoadJson_Delegates()
        {
            const string json = "{\"score\": 99}";
            using var db = Dix.LoadJson(json).OrThrow();
            db.GetInt("score").OrThrow().Should().Be(99);
        }

        // ── ToJson round-trip ──────────────────────────────────────────────────

        [Fact]
        public void ToJson_ThenFromJson_RoundTrips()
        {
            using var original = Load("@DATA( port = 8080, host = \"localhost\", enabled = true )");

            var json  = MdixConverter.ToJson(original, indented: false).OrThrow();
            _out.WriteLine($"JSON: {json}");

            using var restored = MdixConverter.FromJson(json).OrThrow();

            restored.GetInt("port").OrThrow().Should().Be(8080);
            restored.GetString("host").OrThrow().Should().Be("localhost");
            restored.GetBool("enabled").OrThrow().Should().BeTrue();
        }

        // ── ToToml ────────────────────────────────────────────────────────────

        [Fact]
        public void ToToml_SimpleValues_ProducesValidToml()
        {
            using var db = Load("@DATA( port = 8080, host = \"localhost\", ssl = true )");
            var result = MdixConverter.ToToml(db);
            _out.WriteLine(result.UnwrapOr("FAILED"));

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().Contain("8080");
            result.SuccessResult.Should().Contain("localhost");
        }

        [Fact]
        public void ToToml_NullDatabase_ReturnsError()
        {
            MdixConverter.ToToml(null!).IsFailure.Should().BeTrue();
        }

        [Fact]
        public void DixFacade_ToToml_Delegates()
        {
            using var db = Load("@DATA( timeout = 5000 )");
            var result = Dix.ToToml(db);
            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().Contain("5000");
        }

        // ── FromToml ──────────────────────────────────────────────────────────

        [Fact]
        public void FromToml_ValidTable_ProducesReadableDatabase()
        {
            const string toml = "port = 8080\nhost = \"localhost\"\nssl = true\n";
            using var db = MdixConverter.FromToml(toml).OrThrow();
            _out.WriteLine($"IsValid: {db.IsValid}  EntryCount: {db.EntryCount}");

            db.IsValid.Should().BeTrue();
            db.GetInt("port").OrThrow().Should().Be(8080);
            db.GetString("host").OrThrow().Should().Be("localhost");
            db.GetBool("ssl").OrThrow().Should().BeTrue();
        }

        [Fact]
        public void FromToml_NestedTable_AccessibleByDottedPath()
        {
            const string toml = "[server]\nport = 9000\nhost = \"srv.local\"\n";
            using var db = MdixConverter.FromToml(toml).OrThrow();
            _out.WriteLine($"EntryCount: {db.EntryCount}");

            db.GetInt("server.port").OrThrow().Should().Be(9000);
            db.GetString("server.host").OrThrow().Should().Be("srv.local");
        }

        [Fact]
        public void FromToml_InvalidToml_ReturnsError()
        {
            var result = MdixConverter.FromToml("[[[[invalid toml");
            _out.WriteLine($"Error: {(result.IsFailure ? result.Error.Message : "none")}");
            result.IsFailure.Should().BeTrue();
        }

        [Fact]
        public void FromToml_EmptyString_ReturnsError()
        {
            MdixConverter.FromToml(string.Empty).IsFailure.Should().BeTrue();
        }

        [Fact]
        public void DixFacade_LoadToml_Delegates()
        {
            const string toml = "retries = 3\n";
            using var db = Dix.LoadToml(toml).OrThrow();
            db.GetInt("retries").OrThrow().Should().Be(3);
        }

        // ── ToToml round-trip ──────────────────────────────────────────────────

        [Fact]
        public void ToToml_ThenFromToml_RoundTrips()
        {
            using var original = Load("@DATA( port = 8080, host = \"localhost\", enabled = true )");

            var toml = MdixConverter.ToToml(original).OrThrow();
            _out.WriteLine($"TOML:\n{toml}");

            using var restored = MdixConverter.FromToml(toml).OrThrow();

            restored.GetInt("port").OrThrow().Should().Be(8080);
            restored.GetString("host").OrThrow().Should().Be("localhost");
            restored.GetBool("enabled").OrThrow().Should().BeTrue();
        }

        // ── FormatSource ──────────────────────────────────────────────────────

        [Fact]
        public void FormatSource_Compact_RemovesTrailingWhitespace()
        {
            const string input = "x = 1   \ny = 2\t\t";
            var result = MdixConverter.FormatSource(input, MdixFormatMode.Compact);
            _out.WriteLine($"Result: [{result.OrThrow()}]");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().NotContain("   ");
        }

        [Fact]
        public void FormatSource_Minified_RemovesAllUnnecessaryWhitespace()
        {
            const string input = "@CONFIG(\n  version -> \"1.0.0\"\n)";
            var result = MdixConverter.FormatSource(input, MdixFormatMode.Minified);
            _out.WriteLine($"Result: [{result.OrThrow()}]");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().NotContain("\n");
        }

        [Fact]
        public void FormatSource_NullSource_ReturnsError()
        {
            MdixConverter.FormatSource(null!).IsFailure.Should().BeTrue();
        }

        // ── MinifySource ──────────────────────────────────────────────────────

        [Fact]
        public void MinifySource_RemovesLineComments()
        {
            const string input = "x = 5 // comment\ny = 10";
            var result = MdixConverter.MinifySource(input);
            _out.WriteLine($"Result: [{result.OrThrow()}]");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().NotContain("//");
            result.SuccessResult.Should().Contain("x=5");
        }

        [Fact]
        public void MinifySource_RemovesBlockComments()
        {
            const string input = "x = 5 /* block */ y = 10";
            var result = MdixConverter.MinifySource(input);

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().NotContain("block");
        }

        [Fact]
        public void MinifySource_PreservesStringContents()
        {
            const string input = "url = \"http://example.com\" // comment";
            var result = MdixConverter.MinifySource(input);

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().Contain("http://example.com");
        }

        [Fact]
        public void DixFacade_Minify_Delegates()
        {
            const string input = "@DATA(\n  x = 1 // comment\n)";
            var result = Dix.Minify(input);
            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().NotContain("//");
        }

        // ── FIX: StripComments -- was exported from mdix-ffi but never wired up ─

        [Fact]
        public void StripComments_RemovesLineComments()
        {
            const string input = "x = 5 // comment\ny = 10";
            var result = MdixConverter.StripComments(input);
            _out.WriteLine($"Result: [{result.OrThrow()}]");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().NotContain("//");
            result.SuccessResult.Should().Contain("x = 5");
            result.SuccessResult.Should().Contain("y = 10");
        }

        [Fact]
        public void StripComments_RemovesBlockComments()
        {
            const string input = "x = 5 /* block */ y = 10";
            var result = MdixConverter.StripComments(input);

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().NotContain("block");
        }

        [Fact]
        public void StripComments_PreservesStringContents()
        {
            const string input = "url = \"http://example.com\" // comment";
            var result = MdixConverter.StripComments(input);

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().Contain("http://example.com");
        }

        [Fact]
        public void StripComments_PreservesFormattingUnlikeMinify()
        {
            // The distinction that motivated adding this alongside MinifySource:
            // comment removal without whitespace collapsing. FormatSource(...,
            // Compact) doesn't give you this either -- it reformats, it doesn't
            // just strip comments.
            const string input = "@DATA(\n    x = 1, // keep me indented\n    y = 2\n)";
            var result = MdixConverter.StripComments(input);
            _out.WriteLine($"Result: [{result.OrThrow()}]");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().NotContain("keep me indented");
            result.SuccessResult.Should().Contain("\n");
            result.SuccessResult.Should().Contain("    x = 1");
        }

        [Fact]
        public void StripComments_NullSource_ReturnsError()
        {
            MdixConverter.StripComments(null!).IsFailure.Should().BeTrue();
        }

        [Fact]
        public void DixFacade_StripComments_Delegates()
        {
            const string input = "@DATA(\n  x = 1 // comment\n)";
            var result = Dix.StripComments(input);
            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().NotContain("//");
        }

        [Fact]
        public void DixFacade_Format_Delegates()
        {
            const string input = "x = 1   \n\n\n\ny = 2   ";
            var result = Dix.Format(input, MdixFormatMode.Compact);
            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().NotContain("   ");
        }
    }
}
