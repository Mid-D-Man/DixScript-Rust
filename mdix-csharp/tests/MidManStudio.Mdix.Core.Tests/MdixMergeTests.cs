using System.Collections.Generic;
using FluentAssertions;
using MidManStudio.Mdix;
using MidManStudio.Mdix.Core;
using Xunit;
using Xunit.Abstractions;

namespace MidManStudio.Mdix.Core.Tests
{
    public class MdixMergeTests
    {
        private readonly ITestOutputHelper _out;

        public MdixMergeTests(ITestOutputHelper output) => _out = output;

        private MdixDatabase Load(string source)
        {
            _out.WriteLine($"Source: {source}");
            return Dix.LoadStr(source).OrThrow();
        }

        // ── Dix.Merge — basic ─────────────────────────────────────────────────

        [Fact]
        public void Merge_DisjointKeys_CombinesBoth()
        {
            using var primary   = Load("@DATA( host = \"localhost\", port = 8080 )");
            using var secondary = Load("@DATA( timeout = 5000, ssl = true )");

            var result = Dix.Merge(primary, secondary);

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            if (result.IsFailure) _out.WriteLine($"Error: {result.Error}");

            result.IsSuccess.Should().BeTrue();

            using var mergedOutcome = result.SuccessResult;
            var merged = mergedOutcome.Database;
            merged.GetString("host").OrThrow().Should().Be("localhost");
            merged.GetInt("port").OrThrow().Should().Be(8080);
            merged.GetInt("timeout").OrThrow().Should().Be(5000);
            merged.GetBool("ssl").OrThrow().Should().BeTrue();
        }

        [Fact]
        public void Merge_PrimaryWins_PrimaryKeyTakesPrecedence()
        {
            using var primary   = Load("@DATA( port = 8080 )");
            using var secondary = Load("@DATA( port = 9090 )");

            var result = Dix.Merge(primary, secondary, MdixMergeStrategy.PrimaryWins);

            result.IsSuccess.Should().BeTrue();

            using var mergedOutcome = result.SuccessResult;
            var merged = mergedOutcome.Database;
            var port = merged.GetInt("port").OrThrow();
            _out.WriteLine($"Port (expect 8080): {port}");
            port.Should().Be(8080);
        }

        [Fact]
        public void Merge_SecondaryWins_SecondaryKeyOverwrites()
        {
            using var primary   = Load("@DATA( port = 8080 )");
            using var secondary = Load("@DATA( port = 9090 )");

            var result = Dix.Merge(primary, secondary, MdixMergeStrategy.SecondaryWins);

            result.IsSuccess.Should().BeTrue();

            using var mergedOutcome = result.SuccessResult;
            var merged = mergedOutcome.Database;
            var port = merged.GetInt("port").OrThrow();
            _out.WriteLine($"Port (expect 9090): {port}");
            port.Should().Be(9090);
        }

        [Fact]
        public void Merge_ThrowOnConflict_ConflictingKeyReturnsError()
        {
            using var primary   = Load("@DATA( port = 8080 )");
            using var secondary = Load("@DATA( port = 9090 )");

            var result = Dix.Merge(primary, secondary, MdixMergeStrategy.ThrowOnConflict);

            _out.WriteLine($"IsFailure: {result.IsFailure}");
            _out.WriteLine($"Error: {(result.IsFailure ? result.Error.Message : "none")}");

            result.IsFailure.Should().BeTrue();
            result.Error.Message.Should().Contain("Conflict");
        }

        [Fact]
        public void Merge_ThrowOnConflict_DisjointKeysSucceeds()
        {
            using var primary   = Load("@DATA( host = \"localhost\" )");
            using var secondary = Load("@DATA( port = 8080 )");

            var result = Dix.Merge(primary, secondary, MdixMergeStrategy.ThrowOnConflict);

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");

            result.IsSuccess.Should().BeTrue();
            using var mergedOutcome = result.SuccessResult;
            var merged = mergedOutcome.Database;
            merged.GetString("host").OrThrow().Should().Be("localhost");
            merged.GetInt("port").OrThrow().Should().Be(8080);
        }

        // ── Dix.Merge — secondary fills gaps ─────────────────────────────────

        [Fact]
        public void Merge_SecondaryHasExtraKeys_ExtraKeysPresent()
        {
            using var primary   = Load("@DATA( port = 8080 )");
            using var secondary = Load("@DATA( port = 9090, host = \"db.local\", ssl = false )");

            var result = Dix.Merge(primary, secondary, MdixMergeStrategy.PrimaryWins);

            result.IsSuccess.Should().BeTrue();

            using var mergedOutcome = result.SuccessResult;
            var merged = mergedOutcome.Database;
            merged.GetInt("port").OrThrow().Should().Be(8080);
            merged.GetString("host").OrThrow().Should().Be("db.local");
            merged.GetBool("ssl").OrThrow().Should().BeFalse();
        }

        // ── Dix.Merge — nested objects ────────────────────────────────────────

        [Fact]
        public void Merge_NestedObjects_PrimaryWins_DeepMergesAndPrimaryKeyWins()
        {
            using var primary   = Load("@DATA( server: host = \"primary.local\", port = 8080 )");
            using var secondary = Load("@DATA( server: host = \"secondary.local\", timeout = 5000 )");

            var result = Dix.Merge(primary, secondary, MdixMergeStrategy.PrimaryWins);

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            if (result.IsFailure) _out.WriteLine($"Error: {result.Error}");

            result.IsSuccess.Should().BeTrue();

            using var mergedOutcome = result.SuccessResult;
            var merged = mergedOutcome.Database;
            _out.WriteLine($"server.host: {merged.GetString("server.host").UnwrapOr("NOT FOUND")}");
            _out.WriteLine($"server.port: {merged.GetInt("server.port").UnwrapOr(-1)}");
            _out.WriteLine($"server.timeout: {merged.GetInt("server.timeout").UnwrapOr(-1)}");

            merged.GetString("server.host").OrThrow().Should().Be("primary.local");
            merged.GetInt("server.port").OrThrow().Should().Be(8080);
            merged.GetInt("server.timeout").OrThrow().Should().Be(5000);
        }

        [Fact]
        public void Merge_NestedObjects_SecondaryWins_SecondaryKeyWins()
        {
            using var primary   = Load("@DATA( server: host = \"primary.local\", port = 8080 )");
            using var secondary = Load("@DATA( server: host = \"secondary.local\", timeout = 5000 )");

            var result = Dix.Merge(primary, secondary, MdixMergeStrategy.SecondaryWins);

            result.IsSuccess.Should().BeTrue();

            using var mergedOutcome = result.SuccessResult;
            var merged = mergedOutcome.Database;
            merged.GetString("server.host").OrThrow().Should().Be("secondary.local");
            merged.GetInt("server.port").OrThrow().Should().Be(8080);
            merged.GetInt("server.timeout").OrThrow().Should().Be(5000);
        }

        // ── Dix.Merge — arrays are atomic ─────────────────────────────────────

        [Fact]
        public void Merge_ArrayConflict_PrimaryWins_ReplaceStrategy_KeepsPrimaryArray()
        {
            using var primary   = Load("@DATA( tags:: \"alpha\", \"beta\" )");
            using var secondary = Load("@DATA( tags:: \"gamma\", \"delta\", \"epsilon\" )");

            // Array strategy now defaults to ConcatDedup (matches the real Rust
            // core's own default) -- explicit Replace here to test the atomic
            // "winner's array entirely replaces the loser's" behavior specifically.
            var result = Dix.Merge(
                primary, secondary,
                MdixMergeStrategy.PrimaryWins,
                MdixArrayMergeStrategy.Replace);

            result.IsSuccess.Should().BeTrue();

            using var mergedOutcome = result.SuccessResult;
            var merged = mergedOutcome.Database;
            var len = merged.GetArrayLength("tags").OrThrow();
            _out.WriteLine($"Array length (expect 2): {len}");
            len.Should().Be(2);
        }

        [Fact]
        public void Merge_ArrayConflict_SecondaryWins_ReplaceStrategy_KeepsSecondaryArray()
        {
            using var primary   = Load("@DATA( tags:: \"alpha\", \"beta\" )");
            using var secondary = Load("@DATA( tags:: \"gamma\", \"delta\", \"epsilon\" )");

            var result = Dix.Merge(
                primary, secondary,
                MdixMergeStrategy.SecondaryWins,
                MdixArrayMergeStrategy.Replace);

            result.IsSuccess.Should().BeTrue();

            using var mergedOutcome = result.SuccessResult;
            var merged = mergedOutcome.Database;
            var len = merged.GetArrayLength("tags").OrThrow();
            _out.WriteLine($"Array length (expect 3): {len}");
            len.Should().Be(3);
        }

        [Fact]
        public void Merge_ArrayConflict_DefaultStrategy_ConcatDedupsBothArrays()
        {
            using var primary   = Load("@DATA( tags:: \"alpha\", \"beta\" )");
            using var secondary = Load("@DATA( tags:: \"beta\", \"gamma\" )");

            // No explicit array strategy -- exercises the new default
            // (ConcatDedup): winner's items first, loser's items appended,
            // exact-duplicate primitives removed. "beta" appears in both, so
            // the combined array should have 3 entries, not 4.
            var result = Dix.Merge(primary, secondary, MdixMergeStrategy.PrimaryWins);

            result.IsSuccess.Should().BeTrue();

            using var mergedOutcome = result.SuccessResult;
            var merged = mergedOutcome.Database;
            var len = merged.GetArrayLength("tags").OrThrow();
            _out.WriteLine($"Array length (expect 3, deduped): {len}");
            len.Should().Be(3);
        }

        // ── Dix.MergeAll ──────────────────────────────────────────────────────

        [Fact]
        public void MergeAll_ThreeDatabases_CombinesAllDisjointKeys()
        {
            using var db1 = Load("@DATA( a = 1 )");
            using var db2 = Load("@DATA( b = 2 )");
            using var db3 = Load("@DATA( c = 3 )");

            var result = Dix.MergeAll(new[] { db1, db2, db3 });

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            if (result.IsFailure) _out.WriteLine($"Error: {result.Error}");

            result.IsSuccess.Should().BeTrue();

            using var mergedOutcome = result.SuccessResult;
            var merged = mergedOutcome.Database;
            merged.GetInt("a").OrThrow().Should().Be(1);
            merged.GetInt("b").OrThrow().Should().Be(2);
            merged.GetInt("c").OrThrow().Should().Be(3);
        }

        [Fact]
        public void MergeAll_ThreeDatabases_PrimaryWins_FirstWinsOnConflict()
        {
            using var db1 = Load("@DATA( port = 1111 )");
            using var db2 = Load("@DATA( port = 2222 )");
            using var db3 = Load("@DATA( port = 3333 )");

            var result = Dix.MergeAll(
                new[] { db1, db2, db3 },
                MdixMergeStrategy.PrimaryWins);

            result.IsSuccess.Should().BeTrue();

            using var mergedOutcome = result.SuccessResult;
            var merged = mergedOutcome.Database;
            var port = merged.GetInt("port").OrThrow();
            _out.WriteLine($"Port (expect 1111): {port}");
            port.Should().Be(1111);
        }

        [Fact]
        public void MergeAll_ThreeDatabases_SecondaryWins_LastWinsOnConflict()
        {
            using var db1 = Load("@DATA( port = 1111 )");
            using var db2 = Load("@DATA( port = 2222 )");
            using var db3 = Load("@DATA( port = 3333 )");

            var result = Dix.MergeAll(
                new[] { db1, db2, db3 },
                MdixMergeStrategy.SecondaryWins);

            result.IsSuccess.Should().BeTrue();

            using var mergedOutcome = result.SuccessResult;
            var merged = mergedOutcome.Database;
            var port = merged.GetInt("port").OrThrow();
            _out.WriteLine($"Port (expect 3333): {port}");
            port.Should().Be(3333);
        }

        [Fact]
        public void MergeAll_SingleDatabase_ReturnsCopyWithSameValues()
        {
            using var db = Load("@DATA( x = 42, name = \"Solo\" )");

            var result = Dix.MergeAll(new[] { db });

            result.IsSuccess.Should().BeTrue();

            using var mergedOutcome = result.SuccessResult;
            var merged = mergedOutcome.Database;
            merged.GetInt("x").OrThrow().Should().Be(42);
            merged.GetString("name").OrThrow().Should().Be("Solo");
        }

        [Fact]
        public void MergeAll_EmptySequence_ReturnsError()
        {
            var result = Dix.MergeAll(new MdixDatabase[0]);

            _out.WriteLine($"IsFailure: {result.IsFailure}");
            _out.WriteLine($"Error: {(result.IsFailure ? result.Error.Message : "none")}");

            result.IsFailure.Should().BeTrue();
            result.Error.Message.Should().Contain("empty");
        }

        // ── Dix.MergeJson ─────────────────────────────────────────────────────

        [Fact]
        public void MergeJson_AddsJsonKeys_ToExistingDatabase()
        {
            using var primary = Load("@DATA( port = 8080, host = \"localhost\" )");
            const string json = "{\"timeout\": 5000, \"ssl\": true}";

            var result = Dix.MergeJson(primary, json);

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            if (result.IsFailure) _out.WriteLine($"Error: {result.Error}");

            result.IsSuccess.Should().BeTrue();

            using var mergedOutcome = result.SuccessResult;
            var merged = mergedOutcome.Database;
            merged.GetInt("port").OrThrow().Should().Be(8080);
            merged.GetString("host").OrThrow().Should().Be("localhost");
            merged.GetInt("timeout").OrThrow().Should().Be(5000);
            merged.GetBool("ssl").OrThrow().Should().BeTrue();
        }

        [Fact]
        public void MergeJson_ConflictPrimaryWins_PrimaryKeyKept()
        {
            using var primary = Load("@DATA( port = 8080 )");
            const string json = "{\"port\": 9999}";

            var result = Dix.MergeJson(primary, json, MdixMergeStrategy.PrimaryWins);

            result.IsSuccess.Should().BeTrue();
            using var mergedOutcome = result.SuccessResult;
            var merged = mergedOutcome.Database;
            merged.GetInt("port").OrThrow().Should().Be(8080);
        }

        [Fact]
        public void MergeJson_NullPrimary_ReturnsError()
        {
            var result = Dix.MergeJson(null!, "{\"x\": 1}");

            result.IsFailure.Should().BeTrue();
        }

        [Fact]
        public void MergeJson_EmptyJson_ReturnsError()
        {
            using var primary = Load("@DATA( x = 1 )");
            var result = Dix.MergeJson(primary, "");

            result.IsFailure.Should().BeTrue();
        }

        // ── Null / disposed guards ─────────────────────────────────────────────

        [Fact]
        public void Merge_NullPrimary_ReturnsError()
        {
            using var secondary = Load("@DATA( x = 1 )");
            var result = Dix.Merge(null!, secondary);

            result.IsFailure.Should().BeTrue();
            _out.WriteLine($"Error: {result.Error.Message}");
        }

        [Fact]
        public void Merge_NullSecondary_ReturnsError()
        {
            using var primary = Load("@DATA( x = 1 )");
            var result = Dix.Merge(primary, null!);

            result.IsFailure.Should().BeTrue();
            _out.WriteLine($"Error: {result.Error.Message}");
        }

        [Fact]
        public void MergeAll_NullInSequence_ReturnsError()
        {
            using var db1 = Load("@DATA( x = 1 )");
            var result = Dix.MergeAll(new MdixDatabase?[] { db1, null! }!);

            result.IsFailure.Should().BeTrue();
            result.Error.Message.Should().Contain("index 1");
        }

        // ── Async ─────────────────────────────────────────────────────────────

        [Fact]
        public async System.Threading.Tasks.Task MergeAsync_DisjointKeys_Succeeds()
        {
            using var primary   = Load("@DATA( a = 1 )");
            using var secondary = Load("@DATA( b = 2 )");

            var result = await Dix.MergeAsync(primary, secondary);

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");

            result.IsSuccess.Should().BeTrue();
            using var mergedOutcome = result.SuccessResult;
            var merged = mergedOutcome.Database;
            merged.GetInt("a").OrThrow().Should().Be(1);
            merged.GetInt("b").OrThrow().Should().Be(2);
        }

        [Fact]
        public async System.Threading.Tasks.Task MergeAllAsync_ThreeDatabases_Succeeds()
        {
            using var db1 = Load("@DATA( x = 10 )");
            using var db2 = Load("@DATA( y = 20 )");
            using var db3 = Load("@DATA( z = 30 )");

            var result = await Dix.MergeAllAsync(new[] { db1, db2, db3 });

            result.IsSuccess.Should().BeTrue();
            using var mergedOutcome = result.SuccessResult;
            var merged = mergedOutcome.Database;
            merged.GetInt("x").OrThrow().Should().Be(10);
            merged.GetInt("y").OrThrow().Should().Be(20);
            merged.GetInt("z").OrThrow().Should().Be(30);
        }
    }
}
