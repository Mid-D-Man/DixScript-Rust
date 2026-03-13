using FluentAssertions;
using MidManStudio.Mdix.Core;
using Xunit;

namespace MidManStudio.Mdix.Core.Tests
{
    /// <summary>
    /// Integration tests requiring the native mdix_ffi library.
    ///
    /// To run locally:
    ///   1. cargo build -p mdix-ffi
    ///   2. Copy the resulting .dll/.so/.dylib next to the test assembly
    ///   3. Remove the Skip attribute below
    ///
    /// All tests are marked Skip so managed-only CI passes without the native binary.
    /// </summary>
    public class MdixDatabaseTests
    {
        private const string Skip =
            "Requires native mdix_ffi — run 'cargo build -p mdix-ffi' " +
            "and copy the library next to the test assembly.";

        [Fact(Skip = Skip)]
        public void LoadStr_ValidSource_Succeeds()
        {
            using var db = Dix.LoadStr("@DATA( x = 1 )").OrThrow();
            db.IsValid.Should().BeTrue();
        }

        [Fact(Skip = Skip)]
        public void LoadStr_EmptySource_ReturnsErr()
        {
            Dix.LoadStr("").IsFailure.Should().BeTrue();
        }

        [Fact(Skip = Skip)]
        public void LoadStr_MalformedSource_ReturnsParseErr()
        {
            var r = Dix.LoadStr("@@@INVALID$$$");
            r.IsFailure.Should().BeTrue();
        }

        [Fact(Skip = Skip)]
        public void GetString_KnownPath_ReturnsValue()
        {
            using var db = Dix.LoadStr("@DATA( greeting = \"hello\" )").OrThrow();
            db.GetString("greeting").OrThrow().Should().Be("hello");
        }

        [Fact(Skip = Skip)]
        public void GetInt_KnownPath_ReturnsValue()
        {
            using var db = Dix.LoadStr("@DATA( port = 8080 )").OrThrow();
            db.GetInt("port").OrThrow().Should().Be(8080);
        }

        [Fact(Skip = Skip)]
        public void GetBool_KnownPath_ReturnsValue()
        {
            using var db = Dix.LoadStr("@DATA( flag = true )").OrThrow();
            db.GetBool("flag").OrThrow().Should().BeTrue();
        }

        [Fact(Skip = Skip)]
        public void GetInt_MissingPath_ReturnsErr()
        {
            using var db = Dix.LoadStr("@DATA( x = 1 )").OrThrow();
            db.GetInt("does_not_exist").IsFailure.Should().BeTrue();
        }

        [Fact(Skip = Skip)]
        public void Exists_PresentPath_ReturnsTrue()
        {
            using var db = Dix.LoadStr("@DATA( x = 1 )").OrThrow();
            db.Exists("x").Should().BeTrue();
        }

        [Fact(Skip = Skip)]
        public void Exists_AbsentPath_ReturnsFalse()
        {
            using var db = Dix.LoadStr("@DATA( x = 1 )").OrThrow();
            db.Exists("missing").Should().BeFalse();
        }

        [Fact(Skip = Skip)]
        public void GetValueType_ReturnsCorrectDiscriminants()
        {
            using var db = Dix.LoadStr(
                "@DATA( n = 42, s = \"hi\", b = true )").OrThrow();
            db.GetValueType("n").Should().Be(MdixValueType.Int);
            db.GetValueType("s").Should().Be(MdixValueType.String);
            db.GetValueType("b").Should().Be(MdixValueType.Bool);
            db.GetValueType("missing").Should().Be(MdixValueType.Unknown);
        }

        [Fact(Skip = Skip)]
        public void Dispose_CalledTwice_DoesNotThrow()
        {
            var db = Dix.LoadStr("@DATA( x = 1 )").OrThrow();
            db.Dispose();
            System.Action act = () => db.Dispose();
            act.Should().NotThrow();
        }

        [Fact(Skip = Skip)]
        public void GetString_AfterDispose_ReturnsDisposedErr()
        {
            var db = Dix.LoadStr("@DATA( x = \"val\" )").OrThrow();
            db.Dispose();
            db.GetString("x").Error.Kind.Should().Be(MdixErrorKind.Disposed);
        }

        [Fact(Skip = Skip)]
        public void AsDynamic_ScalarAccess_Works()
        {
            using var db = Dix.LoadStr("@DATA( port = 9000 )").OrThrow();
            dynamic cfg = db.AsDynamic();
            int port = cfg.port;
            port.Should().Be(9000);
        }

        [Fact(Skip = Skip)]
        public void Validate_PassesForMatchingSchema()
        {
            using var db = Dix.LoadStr("@DATA( port = 8080 )").OrThrow();
            db.Validate(new MdixSchemaBuilder().RequireInt("port"))
              .IsValid.Should().BeTrue();
        }

        [Fact(Skip = Skip)]
        public void Validate_FailsForMissingRequiredField()
        {
            using var db = Dix.LoadStr("@DATA( x = 1 )").OrThrow();
            var report = db.Validate(new MdixSchemaBuilder().RequireString("missing"));
            report.IsValid.Should().BeFalse();
            report.Errors.Should().HaveCount(1);
            report.Errors[0].Kind.Should().Be(MdixValidationErrorKind.Missing);
        }

        [Fact(Skip = Skip)]
        public void Validate_FailsForTypeMismatch()
        {
            using var db = Dix.LoadStr("@DATA( port = 8080 )").OrThrow();
            var report = db.Validate(new MdixSchemaBuilder().RequireString("port"));
            report.IsValid.Should().BeFalse();
            report.Errors[0].Kind.Should().Be(MdixValidationErrorKind.WrongType);
        }
    }
}
