using System;
using FluentAssertions;
using MidManStudio.Mdix.Core;
using Xunit;
using Xunit.Abstractions;

namespace MidManStudio.Mdix.Core.Tests
{
    public class MdixBuilderTests
    {
        private readonly ITestOutputHelper _out;

        public MdixBuilderTests(ITestOutputHelper output)
        {
            _out = output;
        }

        private static string Serialize(Action<MdixBuilder> configure)
        {
            using var b = MdixBuilder.Create();
            configure(b);
            return b.Serialize().OrThrow();
        }

        // ── Construction ──────────────────────────────────────────────────────

        [Fact]
        public void Create_ReturnsUsableBuilder()
        {
            using var b = MdixBuilder.Create();
            var result = b.Serialize();
            _out.WriteLine($"Builder created: {b != null}");
            _out.WriteLine($"Serialize success: {result.IsSuccess}");
            _out.WriteLine($"Serialize output: \"{result.OrThrow()}\"");
            b.Should().NotBeNull();
            result.IsSuccess.Should().BeTrue();
        }

        // ── @CONFIG ───────────────────────────────────────────────────────────

        [Fact]
        public void Config_Version_AppearsInOutput()
        {
            var s = Serialize(b => b.Config(c => c.WithVersion("1.0.0")));
            _out.WriteLine("Serialized output:");
            _out.WriteLine(s);
            s.Should().Contain("@CONFIG(");
            s.Should().Contain("version");
            s.Should().Contain("1.0.0");
        }

        [Fact]
        public void Config_Author_AppearsInOutput()
        {
            var s = Serialize(b => b.Config(c => c.WithAuthor("MidManStudio")));
            _out.WriteLine("Serialized output:");
            _out.WriteLine(s);
            s.Should().Contain("author");
            s.Should().Contain("MidManStudio");
        }

        [Fact]
        public void Config_Custom_AppearsInOutput()
        {
            var s = Serialize(b => b.Config(c => c.WithCustom("my_key", "my_val")));
            _out.WriteLine("Serialized output:");
            _out.WriteLine(s);
            s.Should().Contain("my_key");
            s.Should().Contain("my_val");
        }

        [Fact]
        public void Config_Created_FormatsAsIso8601()
        {
            var dt = new DateTime(2025, 6, 15, 12, 0, 0, DateTimeKind.Utc);
            var s = Serialize(b => b.Config(c => c.WithCreated(dt)));
            _out.WriteLine($"Input DateTime: {dt:O}");
            _out.WriteLine("Serialized output:");
            _out.WriteLine(s);
            s.Should().Contain("2025-06-15");
        }

        // ── @ENUMS ────────────────────────────────────────────────────────────

        [Fact]
        public void Enums_AutoIncrement_AppearsInOutput()
        {
            var s = Serialize(b =>
                b.Enums(e => e.WithEnum("LogLevel", "DEBUG", "INFO", "WARN")));
            _out.WriteLine("Serialized output:");
            _out.WriteLine(s);
            s.Should().Contain("@ENUMS(");
            s.Should().Contain("LogLevel");
            s.Should().Contain("DEBUG");
            s.Should().Contain("INFO");
            s.Should().Contain("WARN");
        }

        [Fact]
        public void Enums_ExplicitValues_AppearsInOutput()
        {
            var s = Serialize(b =>
                b.Enums(e => e.WithEnum("HttpStatus", ("OK", 200), ("NOT_FOUND", 404))));
            _out.WriteLine("Serialized output:");
            _out.WriteLine(s);
            s.Should().Contain("OK = 200");
            s.Should().Contain("NOT_FOUND = 404");
        }

        [Fact]
        public void Enums_EmptyFieldList_ThrowsArgumentException()
        {
            ArgumentException? caught = null;
            try
            {
                Serialize(b => b.Enums(e => e.WithEnum("Empty")));
            }
            catch (ArgumentException ex)
            {
                caught = ex;
            }
            _out.WriteLine($"Exception type: {caught?.GetType().Name ?? "none"}");
            _out.WriteLine($"Exception message: {caught?.Message ?? "none"}");
            caught.Should().NotBeNull();
        }

        // ── @DATA — flat properties ───────────────────────────────────────────

        [Fact]
        public void Data_WithString_ProducesQuotedValue()
        {
            var s = Serialize(b => b.Data(d => d.WithString("app", "MyApp")));
            _out.WriteLine("Serialized output:");
            _out.WriteLine(s);
            s.Should().Contain("@DATA(");
            s.Should().Contain("app = \"MyApp\"");
        }

        [Fact]
        public void Data_WithInt_ProducesIntegerLiteral()
        {
            var s = Serialize(b => b.Data(d => d.WithInt("port", 8080)));
            _out.WriteLine("Serialized output:");
            _out.WriteLine(s);
            s.Should().Contain("port = 8080");
        }

        [Fact]
        public void Data_WithFloat_ProducesFSuffix()
        {
            var s = Serialize(b => b.Data(d => d.WithFloat("rate", 1.5f)));
            _out.WriteLine("Serialized output:");
            _out.WriteLine(s);
            s.Should().Contain("1.5f");
        }

        [Fact]
        public void Data_WithDouble_ProducesNoFSuffix()
        {
            var s = Serialize(b => b.Data(d => d.WithDouble("price", 19.99)));
            _out.WriteLine("Serialized output:");
            _out.WriteLine(s);
            s.Should().Contain("19.99");
            s.Should().NotContain("19.99f");
        }

        [Fact]
        public void Data_WithBool_ProducesLiterals()
        {
            var s = Serialize(b => b.Data(d =>
                d.WithBool("on", true).WithBool("off", false)));
            _out.WriteLine("Serialized output:");
            _out.WriteLine(s);
            s.Should().Contain("on = true");
            s.Should().Contain("off = false");
        }

        [Fact]
        public void Data_WithHexColor_ProducesUnquotedHex()
        {
            var s = Serialize(b => b.Data(d => d.WithHexColor("primary", "#FF5733")));
            _out.WriteLine("Serialized output:");
            _out.WriteLine(s);
            s.Should().Contain("primary = #FF5733");
        }

        [Fact]
        public void Data_WithHexColor_RejectsNonHashPrefix()
        {
            ArgumentException? caught = null;
            try
            {
                Serialize(b => b.Data(d => d.WithHexColor("c", "FF5733")));
            }
            catch (ArgumentException ex)
            {
                caught = ex;
            }
            _out.WriteLine($"Exception type: {caught?.GetType().Name ?? "none"}");
            _out.WriteLine($"Exception message: {caught?.Message ?? "none"}");
            caught.Should().NotBeNull();
        }

        [Fact]
        public void Data_WithDate_ProducesDateFormat()
        {
            var s = Serialize(b => b.Data(d =>
                d.WithDate("release", new DateTime(2025, 12, 31))));
            _out.WriteLine("Serialized output:");
            _out.WriteLine(s);
            s.Should().Contain("release = 2025-12-31");
        }

        [Fact]
        public void Data_WithBlob_ProducesBlobSyntax()
        {
            var s = Serialize(b => b.Data(d => d.WithBlob("data", "SGVsbG8=")));
            _out.WriteLine("Serialized output:");
            _out.WriteLine(s);
            s.Should().Contain("data = b:(\"SGVsbG8=\")");
        }

        [Fact]
        public void Data_WithBlob_RejectsInvalidBase64()
        {
            ArgumentException? caught = null;
            try
            {
                Serialize(b => b.Data(d => d.WithBlob("x", "not!!base64!!")));
            }
            catch (ArgumentException ex)
            {
                caught = ex;
            }
            _out.WriteLine($"Exception type: {caught?.GetType().Name ?? "none"}");
            _out.WriteLine($"Exception message: {caught?.Message ?? "none"}");
            caught.Should().NotBeNull();
        }

        [Fact]
        public void Data_WithRegex_ProducesRegexSyntax()
        {
            var s = Serialize(b => b.Data(d => d.WithRegex("pat", "^[a-z]+$")));
            _out.WriteLine("Serialized output:");
            _out.WriteLine(s);
            s.Should().Contain("pat = r:(\"^[a-z]+$\")");
        }

        [Fact]
        public void Data_WithEnum_ProducesDotNotation()
        {
            var s = Serialize(b => b.Data(d => d.WithEnum("level", "LogLevel", "INFO")));
            _out.WriteLine("Serialized output:");
            _out.WriteLine(s);
            s.Should().Contain("level = LogLevel.INFO");
        }

        [Fact]
        public void Data_WithTuple_ProducesTupleSyntax()
        {
            var s = Serialize(b => b.Data(d => d.WithTuple("coords", 1, 2, 3)));
            _out.WriteLine("Serialized output:");
            _out.WriteLine(s);
            s.Should().Contain("coords = t:(1, 2, 3)");
        }

        // Tuples max 6 elements. 5 is valid, 7 throws.

        [Fact]
        public void Data_WithTuple_FiveElements_Succeeds()
        {
            var ex = Record.Exception(() =>
                Serialize(b => b.Data(d => d.WithTuple("t", 1, 2, 3, 4, 5))));
            _out.WriteLine($"Exception: {ex?.Message ?? "none"}");
            ex.Should().BeNull();
        }

        [Fact]
        public void Data_WithTuple_SixElements_Succeeds()
        {
            var ex = Record.Exception(() =>
                Serialize(b => b.Data(d => d.WithTuple("t", 1, 2, 3, 4, 5, 6))));
            _out.WriteLine($"Exception: {ex?.Message ?? "none"}");
            ex.Should().BeNull();
        }

        [Fact]
        public void Data_WithTuple_SevenElements_ThrowsArgumentException()
        {
            ArgumentException? caught = null;
            try
            {
                Serialize(b => b.Data(d => d.WithTuple("t", 1, 2, 3, 4, 5, 6, 7)));
            }
            catch (ArgumentException ex)
            {
                caught = ex;
            }
            _out.WriteLine($"Exception type: {caught?.GetType().Name ?? "none"}");
            _out.WriteLine($"Exception message: {caught?.Message ?? "none"}");
            caught.Should().NotBeNull();
        }

        [Fact]
        public void Data_WithArray_ProducesArrayLiteral()
        {
            var s = Serialize(b => b.Data(d => d.WithArray("ids", new[] { 1, 2, 3 })));
            _out.WriteLine("Serialized output:");
            _out.WriteLine(s);
            s.Should().Contain("ids = [1, 2, 3]");
        }

        [Fact]
        public void Data_WithObject_ProducesObjectLiteral()
        {
            var s = Serialize(b => b.Data(d =>
                d.WithObject("cfg", o =>
                    o.WithString("host", "localhost").WithInt("port", 8080))));
            _out.WriteLine("Serialized output:");
            _out.WriteLine(s);
            s.Should().Contain("cfg = {");
            s.Should().Contain("host = \"localhost\"");
            s.Should().Contain("port = 8080");
        }

        // ── @DATA — grouped ───────────────────────────────────────────────────

        [Fact]
        public void Data_WithTableProperties_ProducesSingleColonSyntax()
        {
            var s = Serialize(b => b.Data(d =>
                d.WithTableProperties("server", t =>
                    t.WithString("host", "localhost").WithInt("port", 8080))));
            _out.WriteLine("Serialized output:");
            _out.WriteLine(s);
            s.Should().Contain("server:");
            s.Should().Contain("host = \"localhost\"");
            s.Should().Contain("port = 8080");
        }

        [Fact]
        public void Data_WithGroupArray_Simple_ProducesDoubleColonSyntax()
        {
            var s = Serialize(b => b.Data(d =>
                d.WithGroupArray("tags", new[] { "alpha", "beta" })));
            _out.WriteLine("Serialized output:");
            _out.WriteLine(s);
            s.Should().Contain("tags::");
            s.Should().Contain("\"alpha\"");
            s.Should().Contain("\"beta\"");
        }

        [Fact]
        public void Data_WithGroupArray_Objects_ProducesMultilineLayout()
        {
            var s = Serialize(b => b.Data(d =>
                d.WithGroupArray("enemies", a => a
                    .AddObject(o => o.WithString("name", "Goblin").WithInt("hp", 50))
                    .AddObject(o => o.WithString("name", "Orc").WithInt("hp", 100)))));
            _out.WriteLine("Serialized output:");
            _out.WriteLine(s);
            s.Should().Contain("enemies::");
            s.Should().Contain("Goblin");
            s.Should().Contain("Orc");
        }

        // ── Two-tier enforcement ──────────────────────────────────────────────

        [Fact]
        public void TwoTier_FlatAfterGrouped_ThrowsImmediately()
        {
            InvalidOperationException? caught = null;
            try
            {
                Serialize(b => b.Data(d =>
                {
                    d.WithTableProperties("server", t => t.WithInt("port", 8080));
                    d.WithString("name", "MyApp");
                }));
            }
            catch (InvalidOperationException ex)
            {
                caught = ex;
            }
            _out.WriteLine($"Exception type: {caught?.GetType().Name ?? "none"}");
            _out.WriteLine($"Exception message: {caught?.Message ?? "none"}");
            caught.Should().NotBeNull();
            caught!.Message.Should().Contain("flat property");
        }

        [Fact]
        public void TwoTier_FlatThenGrouped_IsValid()
        {
            var s = Serialize(b => b.Data(d =>
            {
                d.WithString("name", "MyApp");
                d.WithTableProperties("server", t => t.WithInt("port", 8080));
            }));
            _out.WriteLine("Serialized output:");
            _out.WriteLine(s);
            s.Should().Contain("name = \"MyApp\"");
            s.Should().Contain("server:");
        }

        // ── Section ordering ──────────────────────────────────────────────────

        [Fact]
        public void AllThreeSections_AppearInCorrectOrder()
        {
            var s = Serialize(b => b
                .Config(c => c.WithVersion("1.0.0"))
                .Enums(e => e.WithEnum("E", "A", "B"))
                .Data(d => d.WithInt("x", 1)));

            var configIdx = s.IndexOf("@CONFIG(", StringComparison.Ordinal);
            var enumsIdx  = s.IndexOf("@ENUMS(",  StringComparison.Ordinal);
            var dataIdx   = s.IndexOf("@DATA(",   StringComparison.Ordinal);

            _out.WriteLine("Serialized output:");
            _out.WriteLine(s);
            _out.WriteLine($"@CONFIG( at index: {configIdx}");
            _out.WriteLine($"@ENUMS(  at index: {enumsIdx}");
            _out.WriteLine($"@DATA(   at index: {dataIdx}");

            configIdx.Should().BeGreaterThanOrEqualTo(0);
            enumsIdx .Should().BeGreaterThan(configIdx);
            dataIdx  .Should().BeGreaterThan(enumsIdx);
        }

        [Fact]
        public void EmptyConfig_IsOmittedFromOutput()
        {
            var s = Serialize(b => b.Data(d => d.WithInt("x", 1)));
            _out.WriteLine("Serialized output:");
            _out.WriteLine(s);
            s.Should().NotContain("@CONFIG(");
        }

        [Fact]
        public void EmptyData_IsOmittedFromOutput()
        {
            var s = Serialize(b => b.Config(c => c.WithVersion("1.0.0")));
            _out.WriteLine("Serialized output:");
            _out.WriteLine(s);
            s.Should().NotContain("@DATA(");
        }

        [Fact]
        public void EmptyBuilder_ProducesEmptyOrWhitespace()
        {
            var s = Serialize(b => { });
            _out.WriteLine($"Output (repr): \"{s}\"");
            _out.WriteLine($"Trimmed length: {s.Trim().Length}");
            s.Trim().Should().BeEmpty();
        }

        // ── Dispose behaviour ─────────────────────────────────────────────────

        [Fact]
        public void Dispose_CalledTwice_DoesNotThrow()
        {
            var b = MdixBuilder.Create();
            b.Dispose();
            Exception? caught = null;
            try { b.Dispose(); } catch (Exception ex) { caught = ex; }
            _out.WriteLine($"Second dispose threw: {caught?.GetType().Name ?? "nothing"}");
            caught.Should().BeNull();
        }

        [Fact]
        public void Serialize_AfterDispose_ThrowsObjectDisposedException()
        {
            var b = MdixBuilder.Create();
            b.Dispose();
            ObjectDisposedException? caught = null;
            try { b.Serialize(); } catch (ObjectDisposedException ex) { caught = ex; }
            _out.WriteLine($"Exception type: {caught?.GetType().Name ?? "none"}");
            _out.WriteLine($"Object name: {caught?.ObjectName ?? "none"}");
            caught.Should().NotBeNull();
        }

        [Fact]
        public void Config_AfterDispose_ThrowsObjectDisposedException()
        {
            var b = MdixBuilder.Create();
            b.Dispose();
            ObjectDisposedException? caught = null;
            try { b.Config(c => c.WithVersion("x")); } catch (ObjectDisposedException ex) { caught = ex; }
            _out.WriteLine($"Exception type: {caught?.GetType().Name ?? "none"}");
            _out.WriteLine($"Object name: {caught?.ObjectName ?? "none"}");
            caught.Should().NotBeNull();
        }

        [Fact]
        public void Data_AfterDispose_ThrowsObjectDisposedException()
        {
            var b = MdixBuilder.Create();
            b.Dispose();
            ObjectDisposedException? caught = null;
            try { b.Data(d => d.WithInt("x", 1)); } catch (ObjectDisposedException ex) { caught = ex; }
            _out.WriteLine($"Exception type: {caught?.GetType().Name ?? "none"}");
            _out.WriteLine($"Object name: {caught?.ObjectName ?? "none"}");
            caught.Should().NotBeNull();
        }
    }
}
