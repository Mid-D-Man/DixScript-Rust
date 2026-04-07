// csharp/tests/MidManStudio.Mdix.Core.Tests/MdixSerializerTests.cs

using System;
using FluentAssertions;
using MidManStudio.Mdix;
using MidManStudio.Mdix.Core;
using Xunit;
using Xunit.Abstractions;

namespace MidManStudio.Mdix.Core.Tests
{
    // ══════════════════════════════════════════════════════════════════════════
    // Test fixture types — declared outside test class so the serializer's
    // type cache is shared across all test methods within the run.
    // ══════════════════════════════════════════════════════════════════════════

    // Plain class — parameterless constructor, snake_case auto-mapping.
    public class PlainConfig
    {
        public string AppName { get; set; } = string.Empty;
        public int    Port    { get; set; }
        public bool   Enabled { get; set; }
    }

    // Explicit path override via [MdixProperty].
    public class ExplicitPathConfig
    {
        [MdixProperty("server.host")]
        public string Host { get; set; } = string.Empty;

        [MdixProperty("server.port")]
        public int Port { get; set; }
    }

    // Class-level prefix via [MdixObject].
    [MdixObject("server")]
    public class PrefixedConfig
    {
        public string Host { get; set; } = string.Empty;
        public int    Port { get; set; }
    }

    // Record with a primary constructor — tests constructor-first path.
    public record ServerRecord(string Host, int Port, bool Ssl);

    // Record with [MdixProperty] on constructor parameters.
    public record MappedRecord(
        [MdixProperty("server.host")] string Host,
        [MdixProperty("server.port")] int    Port);

    // Struct — tests boxing/unboxing correctness.
    public struct PointStruct
    {
        public int X { get; set; }
        public int Y { get; set; }
    }

    // Nested composition.
    [MdixObject("app")]
    public class AppConfig
    {
        public string Name    { get; set; } = string.Empty;
        public int    Version { get; set; }

        [MdixProperty("server")]
        public ServerSection Server { get; set; } = new();
    }

    [MdixObject("server")]
    public class ServerSection
    {
        public string Host { get; set; } = string.Empty;
        public int    Port { get; set; }
    }

    // [MdixAlias] fallback.
    public class AliasConfig
    {
        [MdixProperty("host")]
        [MdixAlias("server.host")]
        [MdixAlias("connection.host")]
        public string Host { get; set; } = string.Empty;
    }

    // [MdixIgnore].
    public class IgnoreConfig
    {
        public string Name { get; set; } = string.Empty;

        [MdixIgnore]
        public string Secret { get; set; } = "should-not-change";
    }

    // [MdixRequired].
    public class RequiredConfig
    {
        [MdixRequired]
        public string ApiKey { get; set; } = string.Empty;
    }

    // [MdixDefaultValue].
    public class DefaultValueConfig
    {
        [MdixDefaultValue(9090)]
        public int Port { get; set; }

        [MdixDefaultValue("localhost")]
        public string Host { get; set; } = string.Empty;
    }

    // Round-trip target — used to verify serialize → deserialize produces identical values.
    public class RoundTripConfig
    {
        public string  Name    { get; set; } = string.Empty;
        public int     Count   { get; set; }
        public double  Rate    { get; set; }
        public bool    Active  { get; set; }
    }

    // ══════════════════════════════════════════════════════════════════════════
    // Tests
    // ══════════════════════════════════════════════════════════════════════════

    public class MdixSerializerTests
    {
        private readonly ITestOutputHelper _out;

        public MdixSerializerTests(ITestOutputHelper output)
        {
            _out = output;
            Dix.ClearSerializerCache();
        }

        // ── Helpers ───────────────────────────────────────────────────────────

        [ThreadStatic]
        private static ITestOutputHelper? _out_static;

        private MdixDatabase Load(string source)
        {
            _out_static = _out;
            _out.WriteLine($"Source:\n{source}");
            return Dix.LoadStr(source).OrThrow();
        }

        // ── Plain class — snake_case auto-mapping ─────────────────────────────

        [Fact]
        public void PlainClass_SnakeCase_MapsCorrectly()
        {
            using var db = Load("@DATA( app_name = \"MyGame\", port = 8080, enabled = true )");

            var result = db.Deserialize<PlainConfig>();

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            _out.WriteLine($"AppName: {(result.IsSuccess ? result.SuccessResult.AppName : result.Error.Message)}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.AppName.Should().Be("MyGame");
            result.SuccessResult.Port.Should().Be(8080);
            result.SuccessResult.Enabled.Should().BeTrue();
        }

        // ── Explicit path via [MdixProperty] ──────────────────────────────────
        // FIX: use table-property colon syntax so the runtime stores "server.host"
        //      and "server.port" as flat dotted keys in flattened_data.

        [Fact]
        public void ExplicitPath_MdixProperty_MapsCorrectly()
        {
            using var db = Load("@DATA( server: host = \"db.local\", port = 5432 )");

            var result = db.Deserialize<ExplicitPathConfig>();

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            _out.WriteLine($"Host: {(result.IsSuccess ? result.SuccessResult.Host : result.Error.Message)}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Host.Should().Be("db.local");
            result.SuccessResult.Port.Should().Be(5432);
        }

        // ── Class-level prefix via [MdixObject] ───────────────────────────────
        // FIX: same — colon syntax for nested keys.

        [Fact]
        public void ClassLevelPrefix_MdixObject_MapsCorrectly()
        {
            using var db = Load("@DATA( server: host = \"api.local\", port = 443 )");

            var result = db.Deserialize<PrefixedConfig>();

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            _out.WriteLine($"Host: {(result.IsSuccess ? result.SuccessResult.Host : result.Error.Message)}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Host.Should().Be("api.local");
            result.SuccessResult.Port.Should().Be(443);
        }

        // ── Explicit prefix overrides [MdixObject] ────────────────────────────
        // FIX: colon syntax under the "db" table path.

        [Fact]
        public void ExplicitPrefix_OverridesMdixObject()
        {
            using var db = Load("@DATA( db: host = \"replica.local\", port = 5433 )");

            // PrefixedConfig has [MdixObject("server")] but we override with "db".
            var result = db.Deserialize<PrefixedConfig>("db");

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            _out.WriteLine($"Host: {(result.IsSuccess ? result.SuccessResult.Host : result.Error.Message)}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Host.Should().Be("replica.local");
            result.SuccessResult.Port.Should().Be(5433);
        }

        // ── Record with primary constructor ───────────────────────────────────

        [Fact]
        public void Record_PrimaryConstructor_MapsCorrectly()
        {
            // ServerRecord(string Host, int Port, bool Ssl) — names map to host, port, ssl.
            using var db = Load("@DATA( host = \"srv.local\", port = 9000, ssl = true )");

            var result = db.Deserialize<ServerRecord>();

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            _out.WriteLine($"Host: {(result.IsSuccess ? result.SuccessResult.Host : result.Error.Message)}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Host.Should().Be("srv.local");
            result.SuccessResult.Port.Should().Be(9000);
            result.SuccessResult.Ssl.Should().BeTrue();
        }

        // ── Record with [MdixProperty] on constructor parameters ───────────────
        // FIX: colon syntax so "server.host" and "server.port" exist as flat keys.

        [Fact]
        public void Record_MdixPropertyOnParam_MapsCorrectly()
        {
            using var db = Load("@DATA( server: host = \"mapped.local\", port = 7777 )");

            var result = db.Deserialize<MappedRecord>();

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            _out.WriteLine($"Host: {(result.IsSuccess ? result.SuccessResult.Host : result.Error.Message)}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Host.Should().Be("mapped.local");
            result.SuccessResult.Port.Should().Be(7777);
        }

        // ── Struct — boxing/unboxing ───────────────────────────────────────────

        [Fact]
        public void Struct_PropertySet_WorksCorrectly()
        {
            using var db = Load("@DATA( x = 10, y = 20 )");

            var result = db.Deserialize<PointStruct>();

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            _out.WriteLine($"X={result.SuccessResult.X} Y={result.SuccessResult.Y}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.X.Should().Be(10);
            result.SuccessResult.Y.Should().Be(20);
        }

        // ── Nested composition ────────────────────────────────────────────────
        // FIX: two separate table properties — "app:" for name/version and
        //      "app.server:" for the nested ServerSection fields.

        [Fact]
        public void Nested_Composition_MapsCorrectly()
        {
            using var db = Load(
                "@DATA( app: name = \"GameApp\", version = 2, app.server: host = \"game.local\", port = 7070 )");

            var result = db.Deserialize<AppConfig>();

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            if (result.IsSuccess)
            {
                _out.WriteLine($"Name: {result.SuccessResult.Name}");
                _out.WriteLine($"Version: {result.SuccessResult.Version}");
                _out.WriteLine($"Server.Host: {result.SuccessResult.Server.Host}");
                _out.WriteLine($"Server.Port: {result.SuccessResult.Server.Port}");
            }
            else
            {
                _out.WriteLine($"Error: {result.Error.Message}");
            }

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Name.Should().Be("GameApp");
            result.SuccessResult.Version.Should().Be(2);
            result.SuccessResult.Server.Host.Should().Be("game.local");
            result.SuccessResult.Server.Port.Should().Be(7070);
        }

        // ── [MdixAlias] — primary path missing, fallback used ─────────────────
        // FIX: colon syntax so "server.host" is a real flat key for the fallback.

        [Fact]
        public void Alias_FallbackPath_UsedWhenPrimaryMissing()
        {
            // "host" is missing — falls back to "server.host".
            using var db = Load("@DATA( server: host = \"fallback.local\" )");

            var result = db.Deserialize<AliasConfig>();

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            _out.WriteLine($"Host: {(result.IsSuccess ? result.SuccessResult.Host : result.Error.Message)}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Host.Should().Be("fallback.local");
        }

        // FIX: flat "host" first (two-tier rule satisfied), then grouped "server:"
        //      so both keys are available for alias resolution.

        [Fact]
        public void Alias_PrimaryPath_UsedWhenPresent()
        {
            // "host" is present — primary should win over alias.
            using var db = Load("@DATA( host = \"primary.local\", server: host = \"alias.local\" )");

            var result = db.Deserialize<AliasConfig>();

            _out.WriteLine($"Host: {(result.IsSuccess ? result.SuccessResult.Host : result.Error.Message)}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Host.Should().Be("primary.local");
        }

        // ── [MdixIgnore] ──────────────────────────────────────────────────────

        [Fact]
        public void Ignore_MarkedProperty_IsNotPopulated()
        {
            using var db = Load("@DATA( name = \"Test\", secret = \"leaked\" )");

            var result = db.Deserialize<IgnoreConfig>();

            _out.WriteLine($"Name: {result.SuccessResult.Name}");
            _out.WriteLine($"Secret: {result.SuccessResult.Secret}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Name.Should().Be("Test");
            // Secret should remain at its initializer value, not "leaked".
            result.SuccessResult.Secret.Should().Be("should-not-change");
        }

        // ── [MdixRequired] ────────────────────────────────────────────────────

        [Fact]
        public void Required_MissingPath_ReturnsFailure()
        {
            using var db = Load("@DATA( unrelated = \"value\" )");

            var result = db.Deserialize<RequiredConfig>();

            _out.WriteLine($"IsFailure: {result.IsFailure}");
            _out.WriteLine($"Error: {(result.IsFailure ? result.Error.Message : "none")}");

            result.IsFailure.Should().BeTrue();
            result.Error.Message.Should().Contain("api_key");
        }

        [Fact]
        public void Required_PresentPath_Succeeds()
        {
            using var db = Load("@DATA( api_key = \"abc123\" )");

            var result = db.Deserialize<RequiredConfig>();

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.ApiKey.Should().Be("abc123");
        }

        // ── [MdixDefaultValue] ────────────────────────────────────────────────

        [Fact]
        public void DefaultValue_MissingPath_UsesDefault()
        {
            using var db = Load("@DATA( x = 1 )");

            var result = db.Deserialize<DefaultValueConfig>();

            _out.WriteLine($"Port: {result.SuccessResult.Port}");
            _out.WriteLine($"Host: {result.SuccessResult.Host}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Port.Should().Be(9090);
            result.SuccessResult.Host.Should().Be("localhost");
        }

        // ── Round-trip: object → builder → string → db → object ───────────────

        [Fact]
        public void RoundTrip_SerializeDeserialize_ProducesIdenticalValues()
        {
            var original = new RoundTripConfig
            {
                Name   = "RoundTripTest",
                Count  = 42,
                Rate   = 3.14,
                Active = true,
            };

            using var builder = MdixBuilder.Create();
            var serResult = builder.Serialize(original);

            _out.WriteLine($"Serialize success: {serResult.IsSuccess}");
            if (serResult.IsFailure)
            {
                _out.WriteLine($"Error: {serResult.Error.Message}");
                serResult.IsSuccess.Should().BeTrue();
                return;
            }

            var mdixString = builder.Serialize().OrThrow();
            _out.WriteLine($"Generated .mdix:\n{mdixString}");

            using var db = Dix.LoadStr(mdixString).OrThrow();
            _out.WriteLine($"Loaded DB IsValid: {db.IsValid}");

            var result = db.Deserialize<RoundTripConfig>();

            _out.WriteLine($"Deserialize IsSuccess: {result.IsSuccess}");
            if (result.IsFailure)
            {
                _out.WriteLine($"Error: {result.Error.Message}");
            }

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Name.Should().Be(original.Name);
            result.SuccessResult.Count.Should().Be(original.Count);
            result.SuccessResult.Rate.Should().BeApproximately(original.Rate, precision: 0.0001);
            result.SuccessResult.Active.Should().Be(original.Active);
        }

        // ── builder.ToDatabase() convenience ──────────────────────────────────

        [Fact]
        public void ToDatabase_ProducesLoadableDatabase()
        {
            using var builder = MdixBuilder.Create();
            builder.Data(d => d
                .WithString("name", "ToDatabaseTest")
                .WithInt("value", 99));

            var dbResult = builder.ToDatabase();

            _out.WriteLine($"ToDatabase IsSuccess: {dbResult.IsSuccess}");

            dbResult.IsSuccess.Should().BeTrue();

            using var db = dbResult.SuccessResult;
            db.GetString("name").OrThrow().Should().Be("ToDatabaseTest");
            db.GetInt("value").OrThrow().Should().Be(99);
        }

        // ── Dix.Deserialize<T> facade ─────────────────────────────────────────

        [Fact]
        public void DixFacade_DeserializeFrom_DelegatesCorrectly()
        {
            using var db = Load("@DATA( app_name = \"FacadeTest\", port = 1234, enabled = false )");

            var result = Dix.DeserializeFrom<PlainConfig>(db);

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            _out.WriteLine($"AppName: {(result.IsSuccess ? result.SuccessResult.AppName : result.Error.Message)}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.AppName.Should().Be("FacadeTest");
            result.SuccessResult.Port.Should().Be(1234);
            result.SuccessResult.Enabled.Should().BeFalse();
        }

        // ── Type cache is stable across calls ─────────────────────────────────

        [Fact]
        public void TypeCache_MultipleDeserializationsOfSameType_Consistent()
        {
            const string src = "@DATA( app_name = \"Cached\", port = 8888, enabled = true )";

            using var db = Load(src);

            var r1 = db.Deserialize<PlainConfig>();
            var r2 = db.Deserialize<PlainConfig>();
            var r3 = db.Deserialize<PlainConfig>();

            _out.WriteLine($"r1 Port: {r1.SuccessResult.Port}");
            _out.WriteLine($"r2 Port: {r2.SuccessResult.Port}");
            _out.WriteLine($"r3 Port: {r3.SuccessResult.Port}");

            r1.IsSuccess.Should().BeTrue();
            r2.IsSuccess.Should().BeTrue();
            r3.IsSuccess.Should().BeTrue();

            r1.SuccessResult.Port.Should().Be(r2.SuccessResult.Port);
            r2.SuccessResult.Port.Should().Be(r3.SuccessResult.Port);
        }

        // ── Tuple guard: 7 elements throws, 6 elements passes ─────────────────

        [Fact]
        public void Builder_WithTuple_SevenElements_ThrowsArgumentException()
        {
            ArgumentException? caught = null;
            try
            {
                using var b = MdixBuilder.Create();
                b.Data(d => d.WithTuple("t", 1, 2, 3, 4, 5, 6, 7));
            }
            catch (ArgumentException ex) { caught = ex; }

            _out.WriteLine($"Exception: {caught?.Message ?? "none"}");
            caught.Should().NotBeNull();
        }

        [Fact]
        public void Builder_WithTuple_SixElements_Succeeds()
        {
            using var b = MdixBuilder.Create();
            var ex = Record.Exception(() =>
                b.Data(d => d.WithTuple("t", 1, 2, 3, 4, 5, 6)));

            _out.WriteLine($"Exception: {ex?.Message ?? "none"}");
            ex.Should().BeNull();

            var s = b.Serialize().OrThrow();
            _out.WriteLine($"Output:\n{s}");
            s.Should().Contain("t:(1, 2, 3, 4, 5, 6)");
        }
    }
}
