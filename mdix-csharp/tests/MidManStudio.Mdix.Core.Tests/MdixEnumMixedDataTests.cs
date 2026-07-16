using FluentAssertions;
using MidManStudio.Mdix.Core;
using Xunit;
using Xunit.Abstractions;

namespace MidManStudio.Mdix.Core.Tests
{
    /// <summary>
    /// MdixDatabaseTests.cs currently has no enum coverage at all -- the only
    /// enum tests in this project (MdixBuilderTests.cs, "Enums_...") cover
    /// serializing an @ENUMS block, not reading enum data back out of a
    /// loaded database. This file closes that gap on the read side, mirroring
    /// the equivalent Python/WASM coverage so all three bindings exercise the
    /// same fixture and assertions.
    ///
    /// Status.PENDING (= 2) and Role.EDITOR (= 1) are deliberately non-zero,
    /// non-conventional-default variants: dixscript's AST resolver
    /// (Runtime/dix_value.rs, ast_value_to_dix_value) silently falls back to
    /// 0 on an enum-table lookup miss, and 0 happens to be a different,
    /// valid-looking variant (ACTIVE/ADMIN) in both enums below. Picking
    /// non-zero variants means that failure mode shows up as an obvious wrong
    /// number instead of hiding behind a coincidentally-correct 0.
    /// </summary>
    public class MdixEnumMixedDataTests
    {
        private readonly ITestOutputHelper _out;

        public MdixEnumMixedDataTests(ITestOutputHelper output)
        {
            _out = output;
        }

        private const string Source = @"
@ENUMS(
  Status { ACTIVE = 1, INACTIVE = 0, PENDING = 2, ARCHIVED = 3 }
  Role   { ADMIN = 0, EDITOR = 1, VIEWER = 2 }
)
@DATA(
  app = ""enum-mixed-data-fixture""

  user:
    name = ""Alice"",
    age<int> = 30,
    score<double> = 98.5,
    active<bool> = true,
    tags = [""admin"", ""verified""],
    status<enum> = Status.PENDING

  user.permissions::
    { role<enum> = Role.EDITOR, scope = ""team"" },
    { role<enum> = Role.ADMIN,  scope = ""global"" }
)";

        // ── Enum alongside sibling fields on the same table property ───────

        [Fact]
        public void SiblingFields_AreUnaffected_ByTheEnumField()
        {
            using var db = Dix.LoadStr(Source).OrThrow();

            db.GetString("user.name").OrThrow().Should().Be("Alice");
            db.GetInt("user.age").OrThrow().Should().Be(30);
            db.GetDouble("user.score").OrThrow().Should().BeApproximately(98.5, 0.0001);
            db.GetBool("user.active").OrThrow().Should().BeTrue();
            db.GetArrayLength("user.tags").OrThrow().Should().Be(2);
        }

        [Fact]
        public void EnumField_ResolvesNameFieldAndValue_Together()
        {
            using var db = Dix.LoadStr(Source).OrThrow();

            db.GetEnumName("user.status").OrThrow().Should().Be("Status");
            db.GetEnumField("user.status").OrThrow().Should().Be("PENDING");

            // PENDING is declared as 2 -- this is the assertion that would
            // actually catch a silent enum-table lookup-miss fallback to 0.
            db.GetEnumValue("user.status").OrThrow().Should().Be(2);
        }

        // ── Enum nested inside a permissions:: group array element ─────────

        [Fact]
        public void FirstGroupArrayElement_ResolvesIndependently()
        {
            using var db = Dix.LoadStr(Source).OrThrow();

            db.GetEnumName("user.permissions[0].role").OrThrow().Should().Be("Role");
            db.GetEnumField("user.permissions[0].role").OrThrow().Should().Be("EDITOR");
            db.GetEnumValue("user.permissions[0].role").OrThrow().Should().Be(1);
            db.GetString("user.permissions[0].scope").OrThrow().Should().Be("team");
        }

        [Fact]
        public void SecondGroupArrayElement_ResolvesIndependently()
        {
            using var db = Dix.LoadStr(Source).OrThrow();

            db.GetEnumField("user.permissions[1].role").OrThrow().Should().Be("ADMIN");
            db.GetEnumValue("user.permissions[1].role").OrThrow().Should().Be(0);
            db.GetString("user.permissions[1].scope").OrThrow().Should().Be("global");
        }

        [Fact]
        public void TopLevelAndNestedEnumFields_DoNotCrossContaminate()
        {
            using var db = Dix.LoadStr(Source).OrThrow();

            var topLevelField = db.GetEnumField("user.status").OrThrow();
            var nestedField   = db.GetEnumField("user.permissions[0].role").OrThrow();
            _out.WriteLine($"top-level field: {topLevelField}, nested field: {nestedField}");
            topLevelField.Should().NotBe(nestedField);

            var topLevelName = db.GetEnumName("user.status").OrThrow();
            var nestedName   = db.GetEnumName("user.permissions[0].role").OrThrow();
            topLevelName.Should().NotBe(nestedName);
        }
    }
}
