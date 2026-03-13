using System;
using FluentAssertions;
using MidManStudio.Mdix.Core;
using Xunit;

namespace MidManStudio.Mdix.Core.Tests
{
    public class MdixBuilderTests
    {
        // Helper — creates, configures, serializes, returns the output string.
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
            b.Should().NotBeNull();
            b.Serialize().IsSuccess.Should().BeTrue();
        }

        // ── @CONFIG ───────────────────────────────────────────────────────────

        [Fact]
        public void Config_Version_AppearsInOutput()
        {
            var s = Serialize(b => b.Config(c => c.WithVersion("1.0.0")));
            s.Should().Contain("@CONFIG(");
            s.Should().Contain("version");
            s.Should().Contain("1.0.0");
        }

        [Fact]
        public void Config_Author_AppearsInOutput()
        {
            var s = Serialize(b => b.Config(c => c.WithAuthor("MidManStudio")));
            s.Should().Contain("author");
            s.Should().Contain("MidManStudio");
        }

        [Fact]
        public void Config_Custom_AppearsInOutput()
        {
            var s = Serialize(b => b.Config(c => c.WithCustom("my_key", "my_val")));
            s.Should().Contain("my_key");
            s.Should().Contain("my_val");
        }

        [Fact]
        public void Config_Created_FormatsAsIso8601()
        {
            var dt = new DateTime(2025, 6, 15, 12, 0, 0, DateTimeKind.Utc);
            Serialize(b => b.Config(c => c.WithCreated(dt)))
                .Should().Contain("2025-06-15");
        }

        // ── @ENUMS ────────────────────────────────────────────────────────────

        [Fact]
        public void Enums_AutoIncrement_AppearsInOutput()
        {
            var s = Serialize(b =>
                b.Enums(e => e.WithEnum("LogLevel", "DEBUG", "INFO", "WARN")));
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
            s.Should().Contain("OK = 200");
            s.Should().Contain("NOT_FOUND = 404");
        }

        [Fact]
        public void Enums_EmptyFieldList_ThrowsArgumentException()
        {
            Action act = () => Serialize(b =>
                b.Enums(e => e.WithEnum("Empty")));
            act.Should().Throw<ArgumentException>();
        }

        // ── @DATA — flat properties ───────────────────────────────────────────

        [Fact]
        public void Data_WithString_ProducesQuotedValue()
        {
            var s = Serialize(b => b.Data(d => d.WithString("app", "MyApp")));
            s.Should().Contain("@DATA(");
            s.Should().Contain("app = \"MyApp\"");
        }

        [Fact]
        public void Data_WithInt_ProducesIntegerLiteral()
        {
            Serialize(b => b.Data(d => d.WithInt("port", 8080)))
                .Should().Contain("port = 8080");
        }

        [Fact]
        public void Data_WithFloat_ProducesFSuffix()
        {
            Serialize(b => b.Data(d => d.WithFloat("rate", 1.5f)))
                .Should().Contain("1.5f");
        }

        [Fact]
        public void Data_WithDouble_ProducesNoFSuffix()
        {
            var s = Serialize(b => b.Data(d => d.WithDouble("price", 19.99)));
            s.Should().Contain("19.99");
            s.Should().NotContain("19.99f");
        }

        [Fact]
        public void Data_WithBool_ProducesLiterals()
        {
            var s = Serialize(b => b.Data(d =>
                d.WithBool("on", true).WithBool("off", false)));
            s.Should().Contain("on = true");
            s.Should().Contain("off = false");
        }

        [Fact]
        public void Data_WithHexColor_ProducesUnquotedHex()
        {
            Serialize(b => b.Data(d => d.WithHexColor("primary", "#FF5733")))
                .Should().Contain("primary = #FF5733");
        }

        [Fact]
        public void Data_WithHexColor_RejectsNonHashPrefix()
        {
            Action act = () => Serialize(b => b.Data(d => d.WithHexColor("c", "FF5733")));
            act.Should().Throw<ArgumentException>();
        }

        [Fact]
        public void Data_WithDate_ProducesDateFormat()
        {
            Serialize(b => b.Data(d =>
                    d.WithDate("release", new DateTime(2025, 12, 31))))
                .Should().Contain("release = 2025-12-31");
        }

        [Fact]
        public void Data_WithBlob_ProducesBlobSyntax()
        {
            Serialize(b => b.Data(d => d.WithBlob("data", "SGVsbG8=")))
                .Should().Contain("data = b:(\"SGVsbG8=\")");
        }

        [Fact]
        public void Data_WithBlob_RejectsInvalidBase64()
        {
            Action act = () => Serialize(b => b.Data(d => d.WithBlob("x", "not!!base64!!")));
            act.Should().Throw<ArgumentException>();
        }

        [Fact]
        public void Data_WithRegex_ProducesRegexSyntax()
        {
            Serialize(b => b.Data(d => d.WithRegex("pat", "^[a-z]+$")))
                .Should().Contain("pat = r:(\"^[a-z]+$\")");
        }

        [Fact]
        public void Data_WithEnum_ProducesDotNotation()
        {
            Serialize(b => b.Data(d => d.WithEnum("level", "LogLevel", "INFO")))
                .Should().Contain("level = LogLevel.INFO");
        }

        [Fact]
        public void Data_WithTuple_ProducesTupleSyntax()
        {
            Serialize(b => b.Data(d => d.WithTuple("coords", 1, 2, 3)))
                .Should().Contain("coords = t:(1, 2, 3)");
        }

        [Fact]
        public void Data_WithTuple_FiveElements_ThrowsArgumentException()
        {
            Action act = () => Serialize(b =>
                b.Data(d => d.WithTuple("t", 1, 2, 3, 4, 5)));
            act.Should().Throw<ArgumentException>();
        }

        [Fact]
        public void Data_WithArray_ProducesArrayLiteral()
        {
            Serialize(b => b.Data(d => d.WithArray("ids", new[] { 1, 2, 3 })))
                .Should().Contain("ids = [1, 2, 3]");
        }

        [Fact]
        public void Data_WithObject_ProducesObjectLiteral()
        {
            var s = Serialize(b => b.Data(d =>
                d.WithObject("cfg", o =>
                    o.WithString("host", "localhost").WithInt("port", 8080))));
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
            s.Should().Contain("server:");
            s.Should().Contain("host = \"localhost\"");
            s.Should().Contain("port = 8080");
        }

        [Fact]
        public void Data_WithGroupArray_Simple_ProducesDoubleColonSyntax()
        {
            var s = Serialize(b => b.Data(d =>
                d.WithGroupArray("tags", new[] { "alpha", "beta" })));
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
            s.Should().Contain("enemies::");
            s.Should().Contain("Goblin");
            s.Should().Contain("Orc");
        }

        // ── Two-tier enforcement ──────────────────────────────────────────────

        [Fact]
        public void TwoTier_FlatAfterGrouped_ThrowsImmediately()
        {
            Action act = () => Serialize(b => b.Data(d =>
            {
                d.WithTableProperties("server", t => t.WithInt("port", 8080));
                d.WithString("name", "MyApp"); // INVALID
            }));
            act.Should().Throw<InvalidOperationException>()
               .WithMessage("*flat property*");
        }

        [Fact]
        public void TwoTier_FlatThenGrouped_IsValid()
        {
            var s = Serialize(b => b.Data(d =>
            {
                d.WithString("name", "MyApp");
                d.WithTableProperties("server", t => t.WithInt("port", 8080));
            }));
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

            configIdx.Should().BeGreaterThanOrEqualTo(0);
            enumsIdx .Should().BeGreaterThan(configIdx);
            dataIdx  .Should().BeGreaterThan(enumsIdx);
        }

        [Fact]
        public void EmptyConfig_IsOmittedFromOutput()
        {
            Serialize(b => b.Data(d => d.WithInt("x", 1)))
                .Should().NotContain("@CONFIG(");
        }

        [Fact]
        public void EmptyData_IsOmittedFromOutput()
        {
            Serialize(b => b.Config(c => c.WithVersion("1.0.0")))
                .Should().NotContain("@DATA(");
        }

        [Fact]
        public void EmptyBuilder_ProducesEmptyOrWhitespace()
        {
            Serialize(b => { }).Trim().Should().BeEmpty();
        }

        // ── Dispose behaviour ─────────────────────────────────────────────────

        [Fact]
        public void Dispose_CalledTwice_DoesNotThrow()
        {
            var b = MdixBuilder.Create();
            b.Dispose();
            Action act = () => b.Dispose();
            act.Should().NotThrow();
        }

        [Fact]
        public void Serialize_AfterDispose_ThrowsObjectDisposedException()
        {
            var b = MdixBuilder.Create();
            b.Dispose();
            Action act = () => b.Serialize();
            act.Should().Throw<ObjectDisposedException>();
        }

        [Fact]
        public void Config_AfterDispose_ThrowsObjectDisposedException()
        {
            var b = MdixBuilder.Create();
            b.Dispose();
            Action act = () => b.Config(c => c.WithVersion("x"));
            act.Should().Throw<ObjectDisposedException>();
        }

        [Fact]
        public void Data_AfterDispose_ThrowsObjectDisposedException()
        {
            var b = MdixBuilder.Create();
            b.Dispose();
            Action act = () => b.Data(d => d.WithInt("x", 1));
            act.Should().Throw<ObjectDisposedException>();
        }
    }
}
