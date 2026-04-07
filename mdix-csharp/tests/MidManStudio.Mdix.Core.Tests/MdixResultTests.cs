using System;
using System.Threading.Tasks;
using FluentAssertions;
using MidManStudio.Mdix.Core;
using MidManStudio.Utilities;
using Xunit;
using Xunit.Abstractions;

namespace MidManStudio.Mdix.Core.Tests
{
    public class MdixResultTests
    {
        private readonly ITestOutputHelper _out;

        public MdixResultTests(ITestOutputHelper output)
        {
            _out = output;
        }

        // ── MdixResult<T> — construction ──────────────────────────────────────

        [Fact]
        public void Ok_SetsIsSuccessTrue()
        {
            var r = MdixResult<int>.Ok(42);
            _out.WriteLine($"IsSuccess: {r.IsSuccess}");
            _out.WriteLine($"IsFailure: {r.IsFailure}");
            r.IsSuccess.Should().BeTrue();
            r.IsFailure.Should().BeFalse();
        }

        [Fact]
        public void Ok_CarriesValue()
        {
            var r = MdixResult<string>.Ok("hello");
            _out.WriteLine($"Value: {r.SuccessResult}");
            r.SuccessResult.Should().Be("hello");
        }

        [Fact]
        public void Err_SetsIsFailureTrue()
        {
            var r = MdixResult<int>.Err(MdixError.NotFound("p"));
            _out.WriteLine($"IsFailure: {r.IsFailure}");
            _out.WriteLine($"IsSuccess: {r.IsSuccess}");
            r.IsFailure.Should().BeTrue();
            r.IsSuccess.Should().BeFalse();
        }

        [Fact]
        public void Err_CarriesError()
        {
            var r = MdixResult<int>.Err(MdixError.NativeError("boom"));
            _out.WriteLine($"Error kind: {r.Error.Kind}");
            _out.WriteLine($"Error message: {r.Error.Message}");
            r.Error.Kind.Should().Be(MdixErrorKind.NativeError);
            r.Error.Message.Should().Contain("boom");
        }

        [Fact]
        public void SuccessResult_ThrowsOnFailure()
        {
            var r = MdixResult<int>.Err(MdixError.NullHandle());
            InvalidOperationException? caught = null;
            try { _ = r.SuccessResult; } catch (InvalidOperationException ex) { caught = ex; }
            _out.WriteLine($"Exception: {caught?.GetType().Name ?? "none"}");
            _out.WriteLine($"Message: {caught?.Message ?? "none"}");
            caught.Should().NotBeNull();
        }

        [Fact]
        public void Error_ThrowsOnSuccess()
        {
            InvalidOperationException? caught = null;
            try { _ = MdixResult<int>.Ok(1).Error; } catch (InvalidOperationException ex) { caught = ex; }
            _out.WriteLine($"Exception: {caught?.GetType().Name ?? "none"}");
            _out.WriteLine($"Message: {caught?.Message ?? "none"}");
            caught.Should().NotBeNull();
        }

        // ── Unwrapping ────────────────────────────────────────────────────────

        [Fact]
        public void OrThrow_ReturnsValueOnSuccess()
        {
            var val = MdixResult<int>.Ok(99).OrThrow();
            _out.WriteLine($"Value: {val}");
            val.Should().Be(99);
        }

        [Fact]
        public void OrThrow_ThrowsMdixExceptionOnFailure()
        {
            MdixException? caught = null;
            try { MdixResult<int>.Err(MdixError.ParseError("bad")).OrThrow(); }
            catch (MdixException ex) { caught = ex; }
            _out.WriteLine($"Exception: {caught?.GetType().Name ?? "none"}");
            _out.WriteLine($"Error kind: {caught?.MdixError.Kind.ToString() ?? "none"}");
            _out.WriteLine($"Message: {caught?.Message ?? "none"}");
            caught.Should().NotBeNull();
            caught!.MdixError.Kind.Should().Be(MdixErrorKind.ParseError);
        }

        [Fact]
        public void UnwrapOr_ReturnsValueOnSuccess()
        {
            var val = MdixResult<int>.Ok(5).UnwrapOr(-1);
            _out.WriteLine($"Value: {val}");
            val.Should().Be(5);
        }

        [Fact]
        public void UnwrapOr_ReturnsFallbackOnFailure()
        {
            var val = MdixResult<int>.Err(MdixError.NotFound("x")).UnwrapOr(-1);
            _out.WriteLine($"Fallback value: {val}");
            val.Should().Be(-1);
        }

        [Fact]
        public void UnwrapOrElse_CallsFactoryOnFailure()
        {
            var called = false;
            MdixResult<int>.Err(MdixError.NullHandle())
                .UnwrapOrElse(e => { called = true; return 0; });
            _out.WriteLine($"Factory called: {called}");
            called.Should().BeTrue();
        }

        [Fact]
        public void UnwrapOrElse_DoesNotCallFactoryOnSuccess()
        {
            var called = false;
            MdixResult<int>.Ok(1).UnwrapOrElse(e => { called = true; return 0; });
            _out.WriteLine($"Factory called: {called}");
            called.Should().BeFalse();
        }

        // ── Branching ─────────────────────────────────────────────────────────

        [Fact]
        public void Match_Action_CallsSuccessBranchOnOk()
        {
            var hit = false;
            MdixResult<int>.Ok(1).Match(_ => hit = true, _ => { });
            _out.WriteLine($"Success branch hit: {hit}");
            hit.Should().BeTrue();
        }

        [Fact]
        public void Match_Action_CallsFailureBranchOnErr()
        {
            var hit = false;
            MdixResult<int>.Err(MdixError.NullHandle()).Match(_ => { }, _ => hit = true);
            _out.WriteLine($"Failure branch hit: {hit}");
            hit.Should().BeTrue();
        }

        [Fact]
        public void Match_Func_ProjectsCorrectBranch()
        {
            var ok  = MdixResult<int>.Ok(3).Match(v => v * 2, _ => -1);
            var err = MdixResult<int>.Err(MdixError.NullHandle()).Match(v => v * 2, _ => -1);
            _out.WriteLine($"Ok branch result:  {ok}  (expected 6)");
            _out.WriteLine($"Err branch result: {err} (expected -1)");
            ok.Should().Be(6);
            err.Should().Be(-1);
        }

        // ── Transformation ────────────────────────────────────────────────────

        [Fact]
        public void Map_TransformsValueOnSuccess()
        {
            var r = MdixResult<int>.Ok(4).Map(v => v.ToString());
            _out.WriteLine($"Mapped value: {r.SuccessResult}");
            r.IsSuccess.Should().BeTrue();
            r.SuccessResult.Should().Be("4");
        }

        [Fact]
        public void Map_ForwardsErrorUnchangedOnFailure()
        {
            var r = MdixResult<int>.Err(MdixError.NotFound("x")).Map(v => v.ToString());
            _out.WriteLine($"IsFailure: {r.IsFailure}");
            _out.WriteLine($"Error kind: {r.Error.Kind}");
            r.IsFailure.Should().BeTrue();
            r.Error.Kind.Should().Be(MdixErrorKind.NotFound);
        }

        [Fact]
        public void AndThen_ChainsResultOnSuccess()
        {
            var r = MdixResult<int>.Ok(10)
                .AndThen(v => MdixResult<string>.Ok($"val={v}"));
            _out.WriteLine($"Chained value: {r.SuccessResult}");
            r.SuccessResult.Should().Be("val=10");
        }

        [Fact]
        public void AndThen_ShortCircuitsOnFailure()
        {
            var called = false;
            MdixResult<int>.Err(MdixError.NullHandle())
                .AndThen(v => { called = true; return MdixResult<string>.Ok(""); });
            _out.WriteLine($"Binder called: {called}");
            called.Should().BeFalse();
        }

        [Fact]
        public void Ensure_PassingPredicate_ReturnsSameOk()
        {
            var r = MdixResult<int>.Ok(5).Ensure(v => v > 0, MdixError.NullHandle());
            _out.WriteLine($"IsSuccess: {r.IsSuccess}");
            r.IsSuccess.Should().BeTrue();
        }

        [Fact]
        public void Ensure_FailingPredicate_ConvertsToErr()
        {
            var r = MdixResult<int>.Ok(5).Ensure(v => v < 0, MdixError.NativeError("out of range"));
            _out.WriteLine($"IsFailure: {r.IsFailure}");
            _out.WriteLine($"Error: {r.Error}");
            r.IsFailure.Should().BeTrue();
        }

        [Fact]
        public void Ensure_OnFailure_IsPassThrough()
        {
            var r = MdixResult<int>.Err(MdixError.NullHandle())
                .Ensure(v => v > 0, MdixError.NativeError("x"));
            _out.WriteLine($"Error kind preserved: {r.Error.Kind}");
            r.Error.Kind.Should().Be(MdixErrorKind.NullHandle);
        }

        [Fact]
        public void Or_ReturnsThisOnSuccess()
        {
            var val = MdixResult<int>.Ok(1).Or(MdixResult<int>.Ok(99)).SuccessResult;
            _out.WriteLine($"Value: {val} (expected 1, not fallback 99)");
            val.Should().Be(1);
        }

        [Fact]
        public void Or_ReturnsFallbackOnFailure()
        {
            var val = MdixResult<int>.Err(MdixError.NullHandle())
                .Or(MdixResult<int>.Ok(99)).SuccessResult;
            _out.WriteLine($"Fallback value: {val}");
            val.Should().Be(99);
        }

        // ── Side effects ──────────────────────────────────────────────────────

        [Fact]
        public void Tap_CallsActionAndReturnsOriginalOnSuccess()
        {
            var hit = false;
            var r = MdixResult<int>.Ok(7).Tap(v => hit = true);
            _out.WriteLine($"Tap called: {hit}");
            _out.WriteLine($"Original value preserved: {r.SuccessResult}");
            hit.Should().BeTrue();
            r.SuccessResult.Should().Be(7);
        }

        [Fact]
        public void Tap_DoesNotCallOnFailure()
        {
            var hit = false;
            MdixResult<int>.Err(MdixError.NullHandle()).Tap(_ => hit = true);
            _out.WriteLine($"Tap called: {hit}");
            hit.Should().BeFalse();
        }

        [Fact]
        public void TapError_CallsActionOnFailure()
        {
            var hit = false;
            MdixResult<int>.Err(MdixError.NullHandle()).TapError(_ => hit = true);
            _out.WriteLine($"TapError called: {hit}");
            hit.Should().BeTrue();
        }

        [Fact]
        public void TapError_DoesNotCallOnSuccess()
        {
            var hit = false;
            MdixResult<int>.Ok(1).TapError(_ => hit = true);
            _out.WriteLine($"TapError called: {hit}");
            hit.Should().BeFalse();
        }

        // ── Implicit conversion ───────────────────────────────────────────────

        [Fact]
        public void ImplicitConversion_FromMdixError_CreatesErr()
        {
            MdixResult<int> r = MdixError.Disposed("TestClass");
            _out.WriteLine($"IsFailure: {r.IsFailure}");
            _out.WriteLine($"Error kind: {r.Error.Kind}");
            r.IsFailure.Should().BeTrue();
            r.Error.Kind.Should().Be(MdixErrorKind.Disposed);
        }

        // ── MdixError — all factory kinds ─────────────────────────────────────

        [Fact]
        public void ErrorFactories_ProduceCorrectKind()
        {
            var cases = new[]
            {
                (MdixError.NotFound("p"),                  MdixErrorKind.NotFound),
                (MdixError.TypeMismatch("p","int","str"),  MdixErrorKind.TypeMismatch),
                (MdixError.NullHandle(),                   MdixErrorKind.NullHandle),
                (MdixError.InvalidPath("p"),               MdixErrorKind.InvalidPath),
                (MdixError.NativeError("m"),               MdixErrorKind.NativeError),
                (MdixError.IoError("m"),                   MdixErrorKind.IoError),
                (MdixError.ParseError("m"),                MdixErrorKind.ParseError),
                (MdixError.SchemaError("m"),               MdixErrorKind.SchemaError),
                (MdixError.Disposed("T"),                  MdixErrorKind.Disposed),
            };

            foreach (var (err, expected) in cases)
            {
                _out.WriteLine($"{expected,-16} → kind={err.Kind}  message=\"{err.Message}\"");
                err.Kind.Should().Be(expected);
            }
        }

        [Fact]
        public void NotFound_IncludesPathInMessage()
        {
            var e = MdixError.NotFound("server.port");
            _out.WriteLine($"Path: {e.Path}");
            _out.WriteLine($"Message: {e.Message}");
            e.Path.Should().Be("server.port");
            e.Message.Should().Contain("server.port");
        }

        // ── Generic Result<TSuccess, TError> ──────────────────────────────────

        [Fact]
        public void GenericResult_Ok_IsSuccess()
        {
            var r = Result.Ok<int, string>(42);
            _out.WriteLine($"IsSuccess: {r.IsSuccess}");
            _out.WriteLine($"Value: {r.SuccessResult}");
            r.IsSuccess.Should().BeTrue();
            r.SuccessResult.Should().Be(42);
        }

        [Fact]
        public void GenericResult_Err_IsFailure()
        {
            var r = Result.Err<int, string>("oops");
            _out.WriteLine($"IsFailure: {r.IsFailure}");
            _out.WriteLine($"Error: {r.Error}");
            r.IsFailure.Should().BeTrue();
            r.Error.Should().Be("oops");
        }

        [Fact]
        public void GenericResult_Map_TransformsSuccess()
        {
            var val = Result.Ok<int, string>(10).Map(v => v * 2).SuccessResult;
            _out.WriteLine($"Mapped value: {val}");
            val.Should().Be(20);
        }

        [Fact]
        public void GenericResult_AndThen_Chains()
        {
            var val = Result.Ok<int, string>(5)
                .AndThen(v => Result.Ok<string, string>($"x={v}"))
                .SuccessResult;
            _out.WriteLine($"Chained value: {val}");
            val.Should().Be("x=5");
        }

        [Fact]
        public void GenericResult_Try_WrapsSuccessAsOk()
        {
            var r = Result.Try(() => 42);
            _out.WriteLine($"IsSuccess: {r.IsSuccess}");
            _out.WriteLine($"Value: {r.SuccessResult}");
            r.IsSuccess.Should().BeTrue();
            r.SuccessResult.Should().Be(42);
        }

        [Fact]
        public void GenericResult_Try_WrapsExceptionAsErr()
        {
            var r = Result.Try<int>(() => throw new InvalidOperationException("boom"));
            _out.WriteLine($"IsFailure: {r.IsFailure}");
            _out.WriteLine($"Error message: {r.Error.Message}");
            r.IsFailure.Should().BeTrue();
            r.Error.Message.Should().Be("boom");
        }

        [Fact]
        public async Task GenericResult_TryAsync_WrapsSuccessAsOk()
        {
            var r = await Result.TryAsync(async () => { await Task.Delay(1); return 99; });
            _out.WriteLine($"IsSuccess: {r.IsSuccess}");
            _out.WriteLine($"Value: {r.SuccessResult}");
            r.IsSuccess.Should().BeTrue();
            r.SuccessResult.Should().Be(99);
        }

        [Fact]
        public async Task GenericResult_TryAsync_WrapsExceptionAsErr()
        {
            var r = await Result.TryAsync<int>(async () =>
            {
                await Task.Delay(1);
                throw new ArgumentException("async boom");
            });
            _out.WriteLine($"IsFailure: {r.IsFailure}");
            _out.WriteLine($"Error message: {r.Error.Message}");
            r.IsFailure.Should().BeTrue();
            r.Error.Message.Should().Be("async boom");
        }

        [Fact]
        public void GenericResult_OrThrow_ThrowsResultUnwrapExceptionOnFailure()
        {
            ResultUnwrapException? caught = null;
            try { Result.Err<int, string>("fail").OrThrow(); }
            catch (ResultUnwrapException ex) { caught = ex; }
            _out.WriteLine($"Exception: {caught?.GetType().Name ?? "none"}");
            _out.WriteLine($"Message: {caught?.Message ?? "none"}");
            caught.Should().NotBeNull();
        }
    }
}
