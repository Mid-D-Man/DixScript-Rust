using FluentAssertions;
using MidManStudio.Mdix.Core;
using Xunit;
using Xunit.Abstractions;

namespace MidManStudio.Mdix.Core.Tests
{
    public class MdixDatabaseTests
    {
        private readonly ITestOutputHelper _out;

        public MdixDatabaseTests(ITestOutputHelper output)
        {
            _out = output;
        }

        [Fact]
        public void LoadStr_ValidSource_Succeeds()
        {
            const string src = "@DATA( x = 1 )";
            _out.WriteLine($"Source: {src}");
            using var db = Dix.LoadStr(src).OrThrow();
            _out.WriteLine($"IsValid: {db.IsValid}");
            _out.WriteLine($"EntryCount: {db.EntryCount}");
            db.IsValid.Should().BeTrue();
        }

        [Fact]
        public void LoadStr_EmptySource_ReturnsErr()
        {
            var result = Dix.LoadStr("");
            _out.WriteLine($"IsFailure: {result.IsFailure}");
            _out.WriteLine($"Error: {(result.IsFailure ? result.Error.ToString() : "none")}");
            result.IsFailure.Should().BeTrue();
        }

        [Fact]
        public void LoadStr_MalformedSource_ReturnsParseErr()
        {
            const string src = "@@@INVALID$$$";
            _out.WriteLine($"Source: {src}");
            var r = Dix.LoadStr(src);
            _out.WriteLine($"IsFailure: {r.IsFailure}");
            _out.WriteLine($"Error: {(r.IsFailure ? r.Error.ToString() : "none")}");
            r.IsFailure.Should().BeTrue();
        }

        [Fact]
        public void GetString_KnownPath_ReturnsValue()
        {
            using var db = Dix.LoadStr("@DATA( greeting = \"hello\" )").OrThrow();
            var result = db.GetString("greeting");
            _out.WriteLine($"Path: greeting");
            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            _out.WriteLine($"Value: {(result.IsSuccess ? result.SuccessResult : result.Error.ToString())}");
            result.OrThrow().Should().Be("hello");
        }

        [Fact]
        public void GetInt_KnownPath_ReturnsValue()
        {
            using var db = Dix.LoadStr("@DATA( port = 8080 )").OrThrow();
            var result = db.GetInt("port");
            _out.WriteLine($"Path: port");
            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            _out.WriteLine($"Value: {(result.IsSuccess ? result.SuccessResult.ToString() : result.Error.ToString())}");
            result.OrThrow().Should().Be(8080);
        }

        [Fact]
        public void GetBool_KnownPath_ReturnsValue()
        {
            using var db = Dix.LoadStr("@DATA( flag = true )").OrThrow();
            var result = db.GetBool("flag");
            _out.WriteLine($"Path: flag");
            _out.WriteLine($"IsSuccess: {result.IsSuccess}");
            _out.WriteLine($"Value: {(result.IsSuccess ? result.SuccessResult.ToString() : result.Error.ToString())}");
            result.OrThrow().Should().BeTrue();
        }

        [Fact]
        public void GetInt_MissingPath_ReturnsErr()
        {
            using var db = Dix.LoadStr("@DATA( x = 1 )").OrThrow();
            var result = db.GetInt("does_not_exist");
            _out.WriteLine($"Path: does_not_exist");
            _out.WriteLine($"IsFailure: {result.IsFailure}");
            _out.WriteLine($"Error: {(result.IsFailure ? result.Error.ToString() : "none")}");
            result.IsFailure.Should().BeTrue();
        }

        [Fact]
        public void Exists_PresentPath_ReturnsTrue()
        {
            using var db = Dix.LoadStr("@DATA( x = 1 )").OrThrow();
            var exists = db.Exists("x");
            _out.WriteLine($"Path: x");
            _out.WriteLine($"Exists: {exists}");
            exists.Should().BeTrue();
        }

        [Fact]
        public void Exists_AbsentPath_ReturnsFalse()
        {
            using var db = Dix.LoadStr("@DATA( x = 1 )").OrThrow();
            var exists = db.Exists("missing");
            _out.WriteLine($"Path: missing");
            _out.WriteLine($"Exists: {exists}");
            exists.Should().BeFalse();
        }

        [Fact]
        public void GetValueType_ReturnsCorrectDiscriminants()
        {
            using var db = Dix.LoadStr(
                "@DATA( n = 42, s = \"hi\", b = true )").OrThrow();

            var tN = db.GetValueType("n");
            var tS = db.GetValueType("s");
            var tB = db.GetValueType("b");
            var tM = db.GetValueType("missing");

            _out.WriteLine($"n  → {tN}  (expected: Int)");
            _out.WriteLine($"s  → {tS}  (expected: String)");
            _out.WriteLine($"b  → {tB}  (expected: Bool)");
            _out.WriteLine($"?  → {tM}  (expected: Unknown)");

            tN.Should().Be(MdixValueType.Int);
            tS.Should().Be(MdixValueType.String);
            tB.Should().Be(MdixValueType.Bool);
            tM.Should().Be(MdixValueType.Unknown);
        }

        [Fact]
        public void Dispose_CalledTwice_DoesNotThrow()
        {
            var db = Dix.LoadStr("@DATA( x = 1 )").OrThrow();
            db.Dispose();
            System.Exception? caught = null;
            try { db.Dispose(); } catch (System.Exception ex) { caught = ex; }
            _out.WriteLine($"Second dispose threw: {caught?.GetType().Name ?? "nothing"}");
            caught.Should().BeNull();
        }

        [Fact]
        public void GetString_AfterDispose_ReturnsDisposedErr()
        {
            var db = Dix.LoadStr("@DATA( x = \"val\" )").OrThrow();
            db.Dispose();
            var result = db.GetString("x");
            _out.WriteLine($"IsFailure: {result.IsFailure}");
            _out.WriteLine($"Error kind: {(result.IsFailure ? result.Error.Kind.ToString() : "none")}");
            _out.WriteLine($"Error message: {(result.IsFailure ? result.Error.Message : "none")}");
            result.Error.Kind.Should().Be(MdixErrorKind.Disposed);
        }

        [Fact]
        public void AsDynamic_ScalarAccess_Works()
        {
            using var db = Dix.LoadStr("@DATA( port = 9000 )").OrThrow();
            dynamic cfg = db.AsDynamic();
            int port = cfg.port;
            _out.WriteLine($"Dynamic path: cfg.port");
            _out.WriteLine($"Value: {port}");
            port.Should().Be(9000);
        }

        [Fact]
        public void Validate_PassesForMatchingSchema()
        {
            using var db = Dix.LoadStr("@DATA( port = 8080 )").OrThrow();
            var report = db.Validate(new MdixSchemaBuilder().RequireInt("port"));
            _out.WriteLine($"IsValid: {report.IsValid}");
            _out.WriteLine($"Error count: {report.Errors.Count}");
            report.IsValid.Should().BeTrue();
        }

        [Fact]
        public void Validate_FailsForMissingRequiredField()
        {
            using var db = Dix.LoadStr("@DATA( x = 1 )").OrThrow();
            var report = db.Validate(new MdixSchemaBuilder().RequireString("missing"));
            _out.WriteLine($"IsValid: {report.IsValid}");
            _out.WriteLine($"Error count: {report.Errors.Count}");
            foreach (var e in report.Errors)
                _out.WriteLine($"  [{e.Kind}] path={e.Path} expected={e.Expected} actual={e.Actual}");
            report.IsValid.Should().BeFalse();
            report.Errors.Should().HaveCount(1);
            report.Errors[0].Kind.Should().Be(MdixValidationErrorKind.Missing);
        }

        [Fact]
        public void Validate_FailsForTypeMismatch()
        {
            using var db = Dix.LoadStr("@DATA( port = 8080 )").OrThrow();
            var report = db.Validate(new MdixSchemaBuilder().RequireString("port"));
            _out.WriteLine($"IsValid: {report.IsValid}");
            _out.WriteLine($"Error count: {report.Errors.Count}");
            foreach (var e in report.Errors)
                _out.WriteLine($"  [{e.Kind}] path={e.Path} expected={e.Expected} actual={e.Actual}");
            report.IsValid.Should().BeFalse();
            report.Errors[0].Kind.Should().Be(MdixValidationErrorKind.WrongType);
        }
    }
}
