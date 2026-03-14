using System;
using System.Collections.Generic;
using FluentAssertions;
using MidManStudio.Mdix;
using MidManStudio.Mdix.Core;
using Xunit;
using Xunit.Abstractions;

namespace MidManStudio.Mdix.Core.Tests
{
    // ── Test fixture types ────────────────────────────────────────────────────

    public class ArrayEnemy
    {
        public string Name   { get; set; } = string.Empty;
        public int    Hp     { get; set; }
        public string AiType { get; set; } = string.Empty;
    }

    public class ArrayServer
    {
        public string Host { get; set; } = string.Empty;
        public int    Port { get; set; }
    }

    // ── Sources ───────────────────────────────────────────────────────────────

    internal static class ArraySources
    {
        // Scalar arrays
        public const string StringArray =
            "@DATA( tags:: \"alpha\", \"beta\", \"gamma\" )";

        public const string IntArray =
            "@DATA( ids:: 1, 2, 3 )";

        public const string BoolArray =
            "@DATA( flags:: true, false, true )";

        // Float and double arrays
        public const string FloatArray =
            "@DATA( rates:: 1.5f, 2.5f, 3.5f )";

        public const string DoubleArray =
            "@DATA( prices:: 9.99, 19.99, 29.99 )";

        // Complex object array — 3 enemies, two AGGRESSIVE, one BOSS
        public const string EnemyArray =
            "@DATA( enemies:: " +
            "{ name = \"Goblin\", hp = 50, ai_type = \"AGGRESSIVE\" }, " +
            "{ name = \"Orc\", hp = 100, ai_type = \"AGGRESSIVE\" }, " +
            "{ name = \"Dragon\", hp = 1000, ai_type = \"BOSS\" } )";

        // Single-item array
        public const string SingleEnemyArray =
            "@DATA( enemies:: { name = \"Goblin\", hp = 50, ai_type = \"AGGRESSIVE\" } )";

        // Mixed flat + grouped for GetAll with named table entries
        // primary and replica are top-level table entries (objects), not an array
        public const string NamedServers =
            "@DATA( " +
            "primary: host = \"db1.local\", port = 5432 " +
            "replica: host = \"db2.local\", port = 5433 )";

        // Wrong type at a path — port is int, not array
        public const string WrongType =
            "@DATA( port = 8080 )";
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    public class MdixArrayAndLinqTests
    {
        private readonly ITestOutputHelper _out;

        public MdixArrayAndLinqTests(ITestOutputHelper output)
        {
            _out = output;
            Dix.ClearSerializerCache();
        }

        private MdixDatabase Load(string source)
        {
            _out.WriteLine($"Source: {source}");
            return Dix.LoadStr(source).OrThrow();
        }

        // ── GetArray — scalar types ───────────────────────────────────────────

        [Fact]
        public void GetArray_StringArray_ReturnsAllItems()
        {
            using var db = Load(ArraySources.StringArray);
            var result   = db.GetArray<string>("tags");

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            if (result.IsSuccess)
                _out.WriteLine($"Items: [{string.Join(", ", result.SuccessResult)}]");
            else
                _out.WriteLine($"Error: {result.Error}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().HaveCount(3);
            result.SuccessResult[0].Should().Be("alpha");
            result.SuccessResult[1].Should().Be("beta");
            result.SuccessResult[2].Should().Be("gamma");
        }

        [Fact]
        public void GetArray_IntArray_ReturnsAllItems()
        {
            using var db = Load(ArraySources.IntArray);
            var result   = db.GetArray<int>("ids");

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            if (result.IsSuccess)
                _out.WriteLine($"Items: [{string.Join(", ", result.SuccessResult)}]");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().HaveCount(3);
            result.SuccessResult[0].Should().Be(1);
            result.SuccessResult[1].Should().Be(2);
            result.SuccessResult[2].Should().Be(3);
        }

        [Fact]
        public void GetArray_BoolArray_ReturnsAllItems()
        {
            using var db = Load(ArraySources.BoolArray);
            var result   = db.GetArray<bool>("flags");

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            if (result.IsSuccess)
                _out.WriteLine($"Items: [{string.Join(", ", result.SuccessResult)}]");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().HaveCount(3);
            result.SuccessResult[0].Should().BeTrue();
            result.SuccessResult[1].Should().BeFalse();
            result.SuccessResult[2].Should().BeTrue();
        }

        [Fact]
        public void GetArray_FloatArray_ReturnsAllItems()
        {
            using var db = Load(ArraySources.FloatArray);
            var result   = db.GetArray<float>("rates");

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            if (result.IsSuccess)
                _out.WriteLine($"Items: [{string.Join(", ", result.SuccessResult)}]");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().HaveCount(3);
            result.SuccessResult[0].Should().BeApproximately(1.5f, 0.001f);
            result.SuccessResult[1].Should().BeApproximately(2.5f, 0.001f);
            result.SuccessResult[2].Should().BeApproximately(3.5f, 0.001f);
        }

        [Fact]
        public void GetArray_DoubleArray_ReturnsAllItems()
        {
            using var db = Load(ArraySources.DoubleArray);
            var result   = db.GetArray<double>("prices");

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().HaveCount(3);
            result.SuccessResult[0].Should().BeApproximately(9.99,  0.001);
            result.SuccessResult[1].Should().BeApproximately(19.99, 0.001);
            result.SuccessResult[2].Should().BeApproximately(29.99, 0.001);
        }

        // ── GetArray — complex object types ──────────────────────────────────

        [Fact]
        public void GetArray_ComplexObjects_DeserializesEachItem()
        {
            using var db = Load(ArraySources.EnemyArray);
            var result   = db.GetArray<ArrayEnemy>("enemies");

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            if (result.IsSuccess)
                foreach (var e in result.SuccessResult)
                    _out.WriteLine($"  Enemy: name={e.Name} hp={e.Hp} ai={e.AiType}");
            else
                _out.WriteLine($"Error: {result.Error}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().HaveCount(3);

            result.SuccessResult[0].Name.Should().Be("Goblin");
            result.SuccessResult[0].Hp.Should().Be(50);
            result.SuccessResult[0].AiType.Should().Be("AGGRESSIVE");

            result.SuccessResult[1].Name.Should().Be("Orc");
            result.SuccessResult[1].Hp.Should().Be(100);

            result.SuccessResult[2].Name.Should().Be("Dragon");
            result.SuccessResult[2].Hp.Should().Be(1000);
            result.SuccessResult[2].AiType.Should().Be("BOSS");
        }

        [Fact]
        public void GetArray_SingleItem_ReturnsSingleElementList()
        {
            using var db = Load(ArraySources.SingleEnemyArray);
            var result   = db.GetArray<ArrayEnemy>("enemies");

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            if (result.IsSuccess)
                _out.WriteLine($"Count: {result.SuccessResult.Count}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().HaveCount(1);
            result.SuccessResult[0].Name.Should().Be("Goblin");
        }

        // ── GetArray — error cases ────────────────────────────────────────────

        [Fact]
        public void GetArray_MissingPath_ReturnsNotFoundError()
        {
            using var db = Load(ArraySources.IntArray);
            var result   = db.GetArray<int>("does_not_exist");

            _out.WriteLine($"IsFailure: {result.IsFailure}");
            _out.WriteLine($"Error kind: {(result.IsFailure ? result.Error.Kind.ToString() : "none")}");

            result.IsFailure.Should().BeTrue();
            result.Error.Kind.Should().Be(MdixErrorKind.NotFound);
        }

        [Fact]
        public void GetArray_WrongType_ReturnsTypeMismatchError()
        {
            using var db = Load(ArraySources.WrongType);
            // port is an int, not an array
            var result   = db.GetArray<int>("port");

            _out.WriteLine($"IsFailure: {result.IsFailure}");
            _out.WriteLine($"Error kind: {(result.IsFailure ? result.Error.Kind.ToString() : "none")}");
            _out.WriteLine($"Error msg:  {(result.IsFailure ? result.Error.Message : "none")}");

            result.IsFailure.Should().BeTrue();
            result.Error.Kind.Should().Be(MdixErrorKind.TypeMismatch);
        }

        [Fact]
        public void GetArray_NullPath_ReturnsInvalidPathError()
        {
            using var db = Load(ArraySources.IntArray);
            var result   = db.GetArray<int>(null!);

            _out.WriteLine($"IsFailure: {result.IsFailure}");
            _out.WriteLine($"Error kind: {(result.IsFailure ? result.Error.Kind.ToString() : "none")}");

            result.IsFailure.Should().BeTrue();
            result.Error.Kind.Should().Be(MdixErrorKind.InvalidPath);
        }

        [Fact]
        public void GetArray_AfterDispose_ThrowsObjectDisposedException()
        {
            var db = Load(ArraySources.IntArray);
            db.Dispose();
            Action act = () => db.GetArray<int>("ids");
            act.Should().Throw<ObjectDisposedException>();
        }

        // ── GetAll ────────────────────────────────────────────────────────────

        [Fact]
        public void GetAll_WithArrayPrefix_DelegatesToGetArray()
        {
            using var db = Load(ArraySources.EnemyArray);
            var result   = db.GetAll<ArrayEnemy>("enemies");

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            if (result.IsSuccess)
                _out.WriteLine($"Count: {result.SuccessResult.Count}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().HaveCount(3);
            result.SuccessResult[0].Name.Should().Be("Goblin");
        }

        [Fact]
        public void GetAll_WithTablePrefix_ReturnsOneItemPerNamedEntry()
        {
            using var db = Load(ArraySources.NamedServers);
            // primary and replica are object entries, GetKeys(null) returns ["primary","replica"]
            var result   = db.GetAll<ArrayServer>(null);

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            if (result.IsSuccess)
                foreach (var s in result.SuccessResult)
                    _out.WriteLine($"  Server: host={s.Host} port={s.Port}");
            else
                _out.WriteLine($"Error: {result.Error}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().HaveCount(2);

            var primary = result.SuccessResult.Find(s => s.Host == "db1.local");
            primary.Should().NotBeNull();
            primary!.Port.Should().Be(5432);

            var replica = result.SuccessResult.Find(s => s.Host == "db2.local");
            replica.Should().NotBeNull();
            replica!.Port.Should().Be(5433);
        }

        [Fact]
        public void GetAll_EmptyPrefix_ReturnsEmptyListForMissingPath()
        {
            using var db = Load(ArraySources.IntArray);
            var result   = db.GetAll<int>("nonexistent_prefix");

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            if (result.IsSuccess)
                _out.WriteLine($"Count: {result.SuccessResult.Count}");

            // No keys under nonexistent_prefix — expect empty list, not an error
            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().BeEmpty();
        }

        // ── QueryFirst ────────────────────────────────────────────────────────

        [Fact]
        public void QueryFirst_NoPredicate_ReturnsFirstItem()
        {
            using var db = Load(ArraySources.EnemyArray);
            var result   = db.QueryFirst<ArrayEnemy>("enemies");

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            _out.WriteLine($"Name: {(result.IsSuccess ? result.SuccessResult.Name : result.Error.Message)}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Name.Should().Be("Goblin");
        }

        [Fact]
        public void QueryFirst_WithPredicate_ReturnsFirstMatch()
        {
            using var db = Load(ArraySources.EnemyArray);
            var result   = db.QueryFirst<ArrayEnemy>("enemies", e => e.AiType == "BOSS");

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            _out.WriteLine($"Name: {(result.IsSuccess ? result.SuccessResult.Name : result.Error.Message)}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Name.Should().Be("Dragon");
            result.SuccessResult.Hp.Should().Be(1000);
        }

        [Fact]
        public void QueryFirst_NoMatch_ReturnsNotFoundError()
        {
            using var db = Load(ArraySources.EnemyArray);
            var result   = db.QueryFirst<ArrayEnemy>("enemies", e => e.Hp > 99999);

            _out.WriteLine($"IsFailure: {result.IsFailure}");
            _out.WriteLine($"Error kind: {(result.IsFailure ? result.Error.Kind.ToString() : "none")}");

            result.IsFailure.Should().BeTrue();
            result.Error.Kind.Should().Be(MdixErrorKind.NotFound);
        }

        [Fact]
        public void QueryFirst_NullPredicate_ThrowsArgumentNullException()
        {
            using var db = Load(ArraySources.EnemyArray);
            Action act = () => db.QueryFirst<ArrayEnemy>("enemies", null!);
            act.Should().Throw<ArgumentNullException>();
        }

        // ── QueryLast ─────────────────────────────────────────────────────────

        [Fact]
        public void QueryLast_NoPredicate_ReturnsLastItem()
        {
            using var db = Load(ArraySources.EnemyArray);
            var result   = db.QueryLast<ArrayEnemy>("enemies");

            _out.WriteLine($"Name: {(result.IsSuccess ? result.SuccessResult.Name : result.Error.Message)}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Name.Should().Be("Dragon");
        }

        [Fact]
        public void QueryLast_WithPredicate_ReturnsLastMatch()
        {
            using var db = Load(ArraySources.EnemyArray);
            // Both Goblin and Orc are AGGRESSIVE — last one is Orc
            var result   = db.QueryLast<ArrayEnemy>("enemies", e => e.AiType == "AGGRESSIVE");

            _out.WriteLine($"Name: {(result.IsSuccess ? result.SuccessResult.Name : result.Error.Message)}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Name.Should().Be("Orc");
        }

        // ── QuerySingle ───────────────────────────────────────────────────────

        [Fact]
        public void QuerySingle_ExactlyOneMatch_ReturnsItem()
        {
            using var db = Load(ArraySources.EnemyArray);
            var result   = db.QuerySingle<ArrayEnemy>("enemies", e => e.AiType == "BOSS");

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Name.Should().Be("Dragon");
        }

        [Fact]
        public void QuerySingle_MultipleMatches_ReturnsError()
        {
            using var db = Load(ArraySources.EnemyArray);
            // Two AGGRESSIVE enemies → error
            var result   = db.QuerySingle<ArrayEnemy>("enemies", e => e.AiType == "AGGRESSIVE");

            _out.WriteLine($"IsFailure: {result.IsFailure}");
            _out.WriteLine($"Error: {(result.IsFailure ? result.Error.Message : "none")}");

            result.IsFailure.Should().BeTrue();
        }

        [Fact]
        public void QuerySingle_NoMatch_ReturnsNotFoundError()
        {
            using var db = Load(ArraySources.EnemyArray);
            var result   = db.QuerySingle<ArrayEnemy>("enemies", e => e.Name == "Nobody");

            result.IsFailure.Should().BeTrue();
            result.Error.Kind.Should().Be(MdixErrorKind.NotFound);
        }

        // ── QueryWhere ────────────────────────────────────────────────────────

        [Fact]
        public void QueryWhere_WithMatchingPredicate_ReturnsFilteredList()
        {
            using var db = Load(ArraySources.EnemyArray);
            var result   = db.QueryWhere<ArrayEnemy>("enemies", e => e.AiType == "AGGRESSIVE");

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            if (result.IsSuccess)
                foreach (var e in result.SuccessResult)
                    _out.WriteLine($"  {e.Name}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().HaveCount(2);
            result.SuccessResult.Should().Contain(e => e.Name == "Goblin");
            result.SuccessResult.Should().Contain(e => e.Name == "Orc");
            result.SuccessResult.Should().NotContain(e => e.Name == "Dragon");
        }

        [Fact]
        public void QueryWhere_NoMatches_ReturnsEmptyList()
        {
            using var db = Load(ArraySources.EnemyArray);
            var result   = db.QueryWhere<ArrayEnemy>("enemies", e => e.Hp > 99999);

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            _out.WriteLine($"Count: {(result.IsSuccess ? result.SuccessResult.Count : -1)}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().BeEmpty();
        }

        [Fact]
        public void QueryWhere_OnIntArray_FiltersCorrectly()
        {
            using var db = Load(ArraySources.IntArray);
            var result   = db.QueryWhere<int>("ids", v => v > 1);

            _out.WriteLine($"Items: [{(result.IsSuccess ? string.Join(", ", result.SuccessResult) : result.Error.Message)}]");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().HaveCount(2);
            result.SuccessResult.Should().Contain(2);
            result.SuccessResult.Should().Contain(3);
        }

        // ── QuerySelect ───────────────────────────────────────────────────────

        [Fact]
        public void QuerySelect_ProjectsToNewType()
        {
            using var db = Load(ArraySources.EnemyArray);
            var result   = db.QuerySelect<ArrayEnemy, string>("enemies", e => e.Name);

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            if (result.IsSuccess)
                _out.WriteLine($"Names: [{string.Join(", ", result.SuccessResult)}]");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().HaveCount(3);
            result.SuccessResult[0].Should().Be("Goblin");
            result.SuccessResult[1].Should().Be("Orc");
            result.SuccessResult[2].Should().Be("Dragon");
        }

        [Fact]
        public void QuerySelect_ProjectsToInt()
        {
            using var db = Load(ArraySources.EnemyArray);
            var result   = db.QuerySelect<ArrayEnemy, int>("enemies", e => e.Hp);

            _out.WriteLine($"HPs: [{(result.IsSuccess ? string.Join(", ", result.SuccessResult) : result.Error.Message)}]");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().BeEquivalentTo(new[] { 50, 100, 1000 });
        }

        // ── QueryCount ────────────────────────────────────────────────────────

        [Fact]
        public void QueryCount_NoPredicate_ReturnsTotalCount()
        {
            using var db = Load(ArraySources.EnemyArray);
            var result   = db.QueryCount<ArrayEnemy>("enemies");

            _out.WriteLine($"Count: {(result.IsSuccess ? result.SuccessResult : -1)}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().Be(3);
        }

        [Fact]
        public void QueryCount_WithPredicate_ReturnsMatchingCount()
        {
            using var db = Load(ArraySources.EnemyArray);
            var result   = db.QueryCount<ArrayEnemy>("enemies", e => e.AiType == "AGGRESSIVE");

            _out.WriteLine($"Count: {(result.IsSuccess ? result.SuccessResult : -1)}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().Be(2);
        }

        [Fact]
        public void QueryCount_NoMatches_ReturnsZero()
        {
            using var db = Load(ArraySources.EnemyArray);
            var result   = db.QueryCount<ArrayEnemy>("enemies", e => e.Hp > 99999);

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().Be(0);
        }

        // ── QueryAny ─────────────────────────────────────────────────────────

        [Fact]
        public void QueryAny_WithMatch_ReturnsTrue()
        {
            using var db = Load(ArraySources.EnemyArray);
            var result   = db.QueryAny<ArrayEnemy>("enemies", e => e.AiType == "BOSS");

            _out.WriteLine($"Any boss: {(result.IsSuccess ? result.SuccessResult : false)}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().BeTrue();
        }

        [Fact]
        public void QueryAny_WithNoMatch_ReturnsFalse()
        {
            using var db = Load(ArraySources.EnemyArray);
            var result   = db.QueryAny<ArrayEnemy>("enemies", e => e.AiType == "PASSIVE");

            _out.WriteLine($"Any passive: {(result.IsSuccess ? result.SuccessResult : false)}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().BeFalse();
        }

        // ── QueryAll ──────────────────────────────────────────────────────────

        [Fact]
        public void QueryAll_AllMatch_ReturnsTrue()
        {
            using var db = Load(ArraySources.EnemyArray);
            // All enemies have hp > 0
            var result   = db.QueryAll<ArrayEnemy>("enemies", e => e.Hp > 0);

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().BeTrue();
        }

        [Fact]
        public void QueryAll_NotAllMatch_ReturnsFalse()
        {
            using var db = Load(ArraySources.EnemyArray);
            // Not all enemies are BOSS
            var result   = db.QueryAll<ArrayEnemy>("enemies", e => e.AiType == "BOSS");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().BeFalse();
        }

        // ── QueryOrderBy ──────────────────────────────────────────────────────

        [Fact]
        public void QueryOrderBy_SortsAscending()
        {
            using var db = Load(ArraySources.EnemyArray);
            var result   = db.QueryOrderBy<ArrayEnemy, int>("enemies", e => e.Hp);

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            if (result.IsSuccess)
                foreach (var e in result.SuccessResult)
                    _out.WriteLine($"  {e.Name} hp={e.Hp}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult[0].Name.Should().Be("Goblin");
            result.SuccessResult[1].Name.Should().Be("Orc");
            result.SuccessResult[2].Name.Should().Be("Dragon");
        }

        [Fact]
        public void QueryOrderByDescending_SortsDescending()
        {
            using var db = Load(ArraySources.EnemyArray);
            var result   = db.QueryOrderByDescending<ArrayEnemy, int>("enemies", e => e.Hp);

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            if (result.IsSuccess)
                foreach (var e in result.SuccessResult)
                    _out.WriteLine($"  {e.Name} hp={e.Hp}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult[0].Name.Should().Be("Dragon");
            result.SuccessResult[1].Name.Should().Be("Orc");
            result.SuccessResult[2].Name.Should().Be("Goblin");
        }

        [Fact]
        public void QueryOrderBy_ByStringKey_SortsAlphabetically()
        {
            using var db = Load(ArraySources.EnemyArray);
            var result   = db.QueryOrderBy<ArrayEnemy, string>("enemies", e => e.Name);

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            if (result.IsSuccess)
                _out.WriteLine($"Order: {string.Join(", ", db.QuerySelect<ArrayEnemy, string>("enemies", e => e.Name).OrThrow())}");

            result.IsSuccess.Should().BeTrue();
            // Alphabetical: Dragon, Goblin, Orc
            result.SuccessResult[0].Name.Should().Be("Dragon");
            result.SuccessResult[1].Name.Should().Be("Goblin");
            result.SuccessResult[2].Name.Should().Be("Orc");
        }

        // ── QueryDistinct ─────────────────────────────────────────────────────

        [Fact]
        public void QueryDistinct_RemovesDuplicatesFromStringArray()
        {
            // alpha, alpha, beta — expect 2 distinct values
            const string src = "@DATA( tags:: \"alpha\", \"alpha\", \"beta\" )";
            using var db     = Load(src);
            var result       = db.QueryDistinct<string>("tags");

            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            if (result.IsSuccess)
                _out.WriteLine($"Distinct: [{string.Join(", ", result.SuccessResult)}]");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().HaveCount(2);
            result.SuccessResult.Should().Contain("alpha");
            result.SuccessResult.Should().Contain("beta");
        }

        // ── QueryTake / QuerySkip ─────────────────────────────────────────────

        [Fact]
        public void QueryTake_ReturnsFirstNItems()
        {
            using var db = Load(ArraySources.EnemyArray);
            var result   = db.QueryTake<ArrayEnemy>("enemies", 2);

            _out.WriteLine($"Count: {(result.IsSuccess ? result.SuccessResult.Count : -1)}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().HaveCount(2);
            result.SuccessResult[0].Name.Should().Be("Goblin");
            result.SuccessResult[1].Name.Should().Be("Orc");
        }

        [Fact]
        public void QuerySkip_ReturnsItemsAfterN()
        {
            using var db = Load(ArraySources.EnemyArray);
            var result   = db.QuerySkip<ArrayEnemy>("enemies", 2);

            _out.WriteLine($"Count: {(result.IsSuccess ? result.SuccessResult.Count : -1)}");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().HaveCount(1);
            result.SuccessResult[0].Name.Should().Be("Dragon");
        }

        [Fact]
        public void QueryTake_Zero_ReturnsEmptyList()
        {
            using var db = Load(ArraySources.EnemyArray);
            var result   = db.QueryTake<ArrayEnemy>("enemies", 0);

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().BeEmpty();
        }

        [Fact]
        public void QueryTake_MoreThanCount_ReturnsAll()
        {
            using var db = Load(ArraySources.EnemyArray);
            var result   = db.QueryTake<ArrayEnemy>("enemies", 100);

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().HaveCount(3);
        }

        // ── Composition ───────────────────────────────────────────────────────

        [Fact]
        public void Composed_FilterThenProject_WorksCorrectly()
        {
            using var db = Load(ArraySources.EnemyArray);

            // Get names of all AGGRESSIVE enemies with hp > 60, sorted alphabetically
            var filtered = db.QueryWhere<ArrayEnemy>("enemies",
                e => e.AiType == "AGGRESSIVE" && e.Hp > 60).OrThrow();

            var names = filtered
                .OrderBy(e => e.Name)
                .Select(e => e.Name)
                .ToList();

            _out.WriteLine($"Filtered names: [{string.Join(", ", names)}]");

            names.Should().HaveCount(1);
            names[0].Should().Be("Orc");
        }

        [Fact]
        public void Composed_GetArray_ThenLinqDirectly_WorksCorrectly()
        {
            using var db = Load(ArraySources.EnemyArray);

            // GetArray returns List<T> — standard LINQ works directly on it
            var enemies = db.GetArray<ArrayEnemy>("enemies").OrThrow();
            var totalHp = enemies.Sum(e => e.Hp);

            _out.WriteLine($"Total HP: {totalHp}");

            totalHp.Should().Be(50 + 100 + 1000);
        }

        [Fact]
        public void Composed_QuerySelect_ThenQueryWhere_WorksWithMap()
        {
            using var db = Load(ArraySources.EnemyArray);

            // Chain using MdixResult.Map for LINQ on the result
            var bossNames = db
                .GetArray<ArrayEnemy>("enemies")
                .Map(list => list
                    .Where(e => e.AiType == "BOSS")
                    .Select(e => e.Name)
                    .ToList());

            _out.WriteLine($"IsSuccess: {bossNames.IsSuccess}");
            _out.WriteLine($"Boss names: [{(bossNames.IsSuccess ? string.Join(", ", bossNames.SuccessResult) : bossNames.Error.Message)}]");

            bossNames.IsSuccess.Should().BeTrue();
            bossNames.SuccessResult.Should().HaveCount(1);
            bossNames.SuccessResult[0].Should().Be("Dragon");
        }
    }
}
