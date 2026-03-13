using System;
using System.Threading.Tasks;
using FluentAssertions;
using MidManStudio.Mdix.Core;
using MidManStudio.Utilities;
using Xunit;

namespace MidManStudio.Mdix.Core.Tests
{
    public class MdixResultTests
    {
        // ── MdixResult<T> — construction ──────────────────────────────────────

        [Fact]
        public void Ok_SetsIsSuccessTrue()
        {
            var r = MdixResult<int>.Ok(42);
            r.IsSuccess.Should().BeTrue();
            r.IsFailure.Should().BeFalse();
        }

        [Fact]
        public void Ok_CarriesValue()
        {
            MdixResult<string>.Ok("hello").SuccessResult.Should().Be("hello");
        }

        [Fact]
        public void Err_SetsIsFailureTrue()
        {
            var r = MdixResult<int>.Err(MdixError.NotFound("p"));
            r.IsFailure.Should().BeTrue();
            r.IsSuccess.Should().BeFalse();
        }

        [Fact]
        public void Err_CarriesError()
        {
            var r = MdixResult<int>.Err(MdixError.NativeError("boom"));
            r.Error.Kind.Should().Be(MdixErrorKind.NativeError);
            r.Error.Message.Should().Contain("boom");
        }

        [Fact]
        public void SuccessResult_ThrowsOnFailure()
        {
            var r = MdixResult<int>.Err(MdixError.NullHandle());
            Action act = () => _ = r.SuccessResult;
            act.Should().Throw<InvalidOperationException>();
        }

        [Fact]
        public void Error_ThrowsOnSuccess()
        {
            Action act = () => _ = MdixResult<int>.Ok(1).Error;
            act.Should().Throw<InvalidOperationException>();
        }

        // ── Unwrapping ────────────────────────────────────────────────────────

        [Fact]
        public void OrThrow_ReturnsValueOnSuccess()
        {
            MdixResult<int>.Ok(99).OrThrow().Should().Be(99);
        }

        [Fact]
        public void OrThrow_ThrowsMdixExceptionOnFailure()
        {
            Action act = () => MdixResult<int>.Err(MdixError.ParseError("bad")).OrThrow();
            act.Should().Throw<MdixException>()
               .Which.MdixError.Kind.Should().Be(MdixErrorKind.ParseError);
        }

        [Fact]
        public void UnwrapOr_ReturnsValueOnSuccess()
        {
            MdixResult<int>.Ok(5).UnwrapOr(-1).Should().Be(5);
        }

        [Fact]
        public void UnwrapOr_ReturnsFallbackOnFailure()
        {
            MdixResult<int>.Err(MdixError.NotFound("x")).UnwrapOr(-1).Should().Be(-1);
        }

        [Fact]
        public void UnwrapOrElse_CallsFactoryOnFailure()
        {
            var called = false;
            MdixResult<int>.Err(MdixError.NullHandle())
                .UnwrapOrElse(e => { called = true; return 0; });
            called.Should().BeTrue();
        }

        [Fact]
        public void UnwrapOrElse_DoesNotCallFactoryOnSuccess()
        {
            var called = false;
            MdixResult<int>.Ok(1).UnwrapOrElse(e => { called = true; return 0; });
            called.Should().BeFalse();
        }

        // ── Branching ─────────────────────────────────────────────────────────

        [Fact]
        public void Match_Action_CallsSuccessBranchOnOk()
        {
            var hit = false;
            MdixResult<int>.Ok(1).Match(_ => hit = true, _ => { });
            hit.Should().BeTrue();
        }

        [Fact]
        public void Match_Action_CallsFailureBranchOnErr()
        {
            var hit = false;
            MdixResult<int>.Err(MdixError.NullHandle()).Match(_ => { }, _ => hit = true);
            hit.Should().BeTrue();
        }

        [Fact]
        public void Match_Func_ProjectsCorrectBranch()
        {
            var ok  = MdixResult<int>.Ok(3).Match(v => v * 2, _ => -1);
            var err = MdixResult<int>.Err(MdixError.NullHandle()).Match(v => v * 2, _ => -1);
            ok.Should().Be(6);
            err.Should().Be(-1);
        }

        // ── Transformation ────────────────────────────────────────────────────

        [Fact]
        public void Map_TransformsValueOnSuccess()
        {
            var r = MdixResult<int>.Ok(4).Map(v => v.ToString());
            r.IsSuccess.Should().BeTrue();
            r.SuccessResult.Should().Be("4");
        }

        [Fact]
        public void Map_ForwardsErrorUnchangedOnFailure()
        {
            var r = MdixResult<int>.Err(MdixError.NotFound("x")).Map(v => v.ToString());
            r.IsFailure.Should().BeTrue();
            r.Error.Kind.Should().Be(MdixErrorKind.NotFound);
        }

        [Fact]
        public void AndThen_ChainsResultOnSuccess()
        {
            var r = MdixResult<int>.Ok(10)
                .AndThen(v => MdixResult<string>.Ok($"val={v}"));
            r.SuccessResult.Should().Be("val=10");
        }

        [Fact]
        public void AndThen_ShortCircuitsOnFailure()
        {
            var called = false;
            MdixResult<int>.Err(MdixError.NullHandle())
                .AndThen(v => { called = true; return MdixResult<string>.Ok(""); });
            called.Should().BeFalse();
        }

        [Fact]
        public void Ensure_PassingPredicate_ReturnsSameOk()
        {
            MdixResult<int>.Ok(5)
                .Ensure(v => v > 0, MdixError.NullHandle())
                .IsSuccess.Should().BeTrue();
        }

        [Fact]
        public void Ensure_FailingPredicate_ConvertsToErr()
        {
            MdixResult<int>.Ok(5)
                .Ensure(v => v < 0, MdixError.NativeError("out of range"))
                .IsFailure.Should().BeTrue();
        }

        [Fact]
        public void Ensure_OnFailure_IsPassThrough()
        {
            var r = MdixResult<int>.Err(MdixError.NullHandle())
                .Ensure(v => v > 0, MdixError.NativeError("x"));
            r.Error.Kind.Should().Be(MdixErrorKind.NullHandle);
        }

        [Fact]
        public void Or_ReturnsThisOnSuccess()
        {
            MdixResult<int>.Ok(1).Or(MdixResult<int>.Ok(99))
                .SuccessResult.Should().Be(1);
        }

        [Fact]
        public void Or_ReturnsFallbackOnFailure()
        {
            MdixResult<int>.Err(MdixError.NullHandle())
                .Or(MdixResult<int>.Ok(99))
                .SuccessResult.Should().Be(99);
        }

        // ── Side effects ──────────────────────────────────────────────────────

        [Fact]
        public void Tap_CallsActionAndReturnsOriginalOnSuccess()
        {
            var hit = false;
            var r = MdixResult<int>.Ok(7).Tap(v => hit = true);
            hit.Should().BeTrue();
            r.SuccessResult.Should().Be(7);
        }

        [Fact]
        public void Tap_DoesNotCallOnFailure()
        {
            var hit = false;
            MdixResult<int>.Err(MdixError.NullHandle()).Tap(_ => hit = true);
            hit.Should().BeFalse();
        }

        [Fact]
        public void TapError_CallsActionOnFailure()
        {
            var hit = false;
            MdixResult<int>.Err(MdixError.NullHandle()).TapError(_ => hit = true);
            hit.Should().BeTrue();
        }

        [Fact]
        public void TapError_DoesNotCallOnSuccess()
        {
            var hit = false;
            MdixResult<int>.Ok(1).TapError(_ => hit = true);
            hit.Should().BeFalse();
        }

        // ── Implicit conversion ───────────────────────────────────────────────

        [Fact]
        public void ImplicitConversion_FromMdixError_CreatesErr()
        {
            MdixResult<int> r = MdixError.Disposed("TestClass");
            r.IsFailure.Should().BeTrue();
            r.Error.Kind.Should().Be(MdixErrorKind.Disposed);
        }

        // ── MdixError — all factory kinds ────────────────────────────────────

        [Fact]
        public void ErrorFactories_ProduceCorrectKind()
        {
            MdixError.NotFound("p").Kind          .Should().Be(MdixErrorKind.NotFound);
            MdixError.TypeMismatch("p","int","str")
                                   .Kind          .Should().Be(MdixErrorKind.TypeMismatch);
            MdixError.NullHandle() .Kind          .Should().Be(MdixErrorKind.NullHandle);
            MdixError.InvalidPath("p").Kind       .Should().Be(MdixErrorKind.InvalidPath);
            MdixError.NativeError("m").Kind       .Should().Be(MdixErrorKind.NativeError);
            MdixError.IoError("m").Kind           .Should().Be(MdixErrorKind.IoError);
            MdixError.ParseError("m").Kind        .Should().Be(MdixErrorKind.ParseError);
            MdixError.SchemaError("m").Kind       .Should().Be(MdixErrorKind.SchemaError);
            MdixError.Disposed("T").Kind          .Should().Be(MdixErrorKind.Disposed);
        }

        [Fact]
        public void NotFound_IncludesPathInMessage()
        {
            var e = MdixError.NotFound("server.port");
            e.Path.Should().Be("server.port");
            e.Message.Should().Contain("server.port");
        }

        // ── Generic Result<TSuccess, TError> ─────────────────────────────────

        [Fact]
        public void GenericResult_Ok_IsSuccess()
        {
            var r = Result.Ok<int, string>(42);
            r.IsSuccess.Should().BeTrue();
            r.SuccessResult.Should().Be(42);
        }

        [Fact]
        public void GenericResult_Err_IsFailure()
        {
            var r = Result.Err<int, string>("oops");
            r.IsFailure.Should().BeTrue();
            r.Error.Should().Be("oops");
        }

        [Fact]
        public void GenericResult_Map_TransformsSuccess()
        {
            Result.Ok<int, string>(10).Map(v => v * 2)
                  .SuccessResult.Should().Be(20);
        }

        [Fact]
        public void GenericResult_AndThen_Chains()
        {
            Result.Ok<int, string>(5)
                  .AndThen(v => Result.Ok<string, string>($"x={v}"))
                  .SuccessResult.Should().Be("x=5");
        }

        [Fact]
        public void GenericResult_Try_WrapsSuccessAsOk()
        {
            var r = Result.Try(() => 42);
            r.IsSuccess.Should().BeTrue();
            r.SuccessResult.Should().Be(42);
        }

        [Fact]
        public void GenericResult_Try_WrapsExceptionAsErr()
        {
            var r = Result.Try<int>(() => throw new InvalidOperationException("boom"));
            r.IsFailure.Should().BeTrue();
            r.Error.Message.Should().Be("boom");
        }

        [Fact]
        public async Task GenericResult_TryAsync_WrapsSuccessAsOk()
        {
            var r = await Result.TryAsync(async () => { await Task.Delay(1); return 99; });
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
            r.IsFailure.Should().BeTrue();
            r.Error.Message.Should().Be("async boom");
        }

        [Fact]
        public void GenericResult_OrThrow_ThrowsResultUnwrapExceptionOnFailure()
        {
            Action act = () => Result.Err<int, string>("fail").OrThrow();
            act.Should().Throw<ResultUnwrapException>();
        }
    }
}
