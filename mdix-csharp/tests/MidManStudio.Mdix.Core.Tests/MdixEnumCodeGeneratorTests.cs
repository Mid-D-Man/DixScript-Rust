using System;
using System.IO;
using FluentAssertions;
using MidManStudio.Mdix.Core;
using Xunit;
using Xunit.Abstractions;

namespace MidManStudio.Mdix.Core.Tests
{
    public class MdixEnumCodeGeneratorTests : IDisposable
    {
        private readonly ITestOutputHelper _out;
        private readonly string _path;

        public MdixEnumCodeGeneratorTests(ITestOutputHelper output)
        {
            _out  = output;
            _path = Path.Combine(Path.GetTempPath(), $"mdix-enumgen-test-{Guid.NewGuid():N}.mdix");
        }

        public void Dispose()
        {
            try { if (File.Exists(_path)) File.Delete(_path); }
            catch { /* best-effort cleanup */ }
        }

        [Fact]
        public void GenerateFromSource_SingleEnum_ExplicitValues_ProducesMatchingEnum()
        {
            const string source = "@ENUMS( LogLevel { DEBUG = 0, INFO = 1, WARN = 2, ERROR = 3 } )";

            var result = MdixEnumCodeGenerator.GenerateFromSource(source, "MyApp.Config");

            _out.WriteLine(result.IsSuccess ? result.SuccessResult : result.Error.Message);

            result.IsSuccess.Should().BeTrue();
            var code = result.SuccessResult;
            code.Should().Contain("namespace MyApp.Config");
            code.Should().Contain("public enum LogLevel");
            code.Should().Contain("DEBUG = 0");
            code.Should().Contain("INFO = 1");
            code.Should().Contain("WARN = 2");
            code.Should().Contain("ERROR = 3");
        }

        [Fact]
        public void GenerateFromSource_MultipleEnums_ProducesBothTypes()
        {
            const string source =
                "@ENUMS( LogLevel { DEBUG = 0, INFO = 1 } Environment { DEV = 1, STAGING = 2, PROD = 3 } )";

            var result = MdixEnumCodeGenerator.GenerateFromSource(source);

            _out.WriteLine(result.IsSuccess ? result.SuccessResult : result.Error.Message);

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().Contain("public enum LogLevel");
            result.SuccessResult.Should().Contain("public enum Environment");
            result.SuccessResult.Should().Contain("PROD = 3");
        }

        [Fact]
        public void GenerateFromSource_ImplicitValues_OmitsExplicitAssignment()
        {
            // Fields with no "= N" in the source should have no "= N" in the
            // generated code either -- C#'s own enum auto-numbering rule
            // (previous value + 1, from 0) is relied on to reproduce
            // DixScript's identical rule, rather than this generator
            // computing the numbers itself.
            const string source = "@ENUMS( Status { PENDING, ACTIVE, DONE = 5, ARCHIVED } )";

            var result = MdixEnumCodeGenerator.GenerateFromSource(source);

            _out.WriteLine(result.IsSuccess ? result.SuccessResult : result.Error.Message);

            result.IsSuccess.Should().BeTrue();
            var code = result.SuccessResult;
            code.Should().Contain("PENDING,");
            code.Should().Contain("ACTIVE,");
            code.Should().Contain("DONE = 5,");
            code.Should().Contain("ARCHIVED");
            code.Should().NotContain("PENDING =");
            code.Should().NotContain("ACTIVE =");
            code.Should().NotContain("ARCHIVED =");
        }

        [Fact]
        public void GenerateFromSource_CommasBetweenFieldsAreOptional()
        {
            // Per DixScript's own grammar -- commas between enum fields are
            // optional. Mixing both in one declaration should parse the same
            // as either used consistently.
            const string source = "@ENUMS( Mixed { A = 1 B = 2, C = 3 } )";

            var result = MdixEnumCodeGenerator.GenerateFromSource(source);

            _out.WriteLine(result.IsSuccess ? result.SuccessResult : result.Error.Message);

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().Contain("A = 1");
            result.SuccessResult.Should().Contain("B = 2");
            result.SuccessResult.Should().Contain("C = 3");
        }

        [Fact]
        public void GenerateFromSource_CommentsInsideSection_AreIgnored()
        {
            const string source =
                "@ENUMS(\n" +
                "    // this is the log level enum\n" +
                "    LogLevel { DEBUG = 0, INFO = 1 /* verbose-ish */ }\n" +
                ")";

            var result = MdixEnumCodeGenerator.GenerateFromSource(source);

            _out.WriteLine(result.IsSuccess ? result.SuccessResult : result.Error.Message);

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().Contain("public enum LogLevel");
            result.SuccessResult.Should().Contain("DEBUG = 0");
            result.SuccessResult.Should().Contain("INFO = 1");
        }

        [Fact]
        public void GenerateFromSource_NegativeValue_IsPreserved()
        {
            const string source = "@ENUMS( Delta { NEGATIVE = -1, ZERO = 0, POSITIVE = 1 } )";

            var result = MdixEnumCodeGenerator.GenerateFromSource(source);

            _out.WriteLine(result.IsSuccess ? result.SuccessResult : result.Error.Message);

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().Contain("NEGATIVE = -1");
        }

        [Fact]
        public void GenerateFromSource_KeywordCollidingName_IsEscaped()
        {
            // "class" and "default" are reserved C# keywords -- a DixScript
            // enum/field with either name should come out as "@class"/
            // "@default" so the generated file actually compiles.
            const string source = "@ENUMS( class { default = 0, other = 1 } )";

            var result = MdixEnumCodeGenerator.GenerateFromSource(source);

            _out.WriteLine(result.IsSuccess ? result.SuccessResult : result.Error.Message);

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().Contain("public enum @class");
            result.SuccessResult.Should().Contain("@default = 0");
        }

        [Fact]
        public void GenerateFromSource_AccessModifierInternal_IsRespected()
        {
            const string source = "@ENUMS( LogLevel { DEBUG = 0 } )";

            var result = MdixEnumCodeGenerator.GenerateFromSource(source, accessModifier: "internal");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().Contain("internal enum LogLevel");
        }

        [Fact]
        public void GenerateFromSource_InvalidAccessModifier_ReturnsError()
        {
            const string source = "@ENUMS( LogLevel { DEBUG = 0 } )";

            var result = MdixEnumCodeGenerator.GenerateFromSource(source, accessModifier: "protected");

            _out.WriteLine(result.IsFailure ? result.Error.Message : "unexpectedly succeeded");
            result.IsFailure.Should().BeTrue();
        }

        [Fact]
        public void GenerateFromSource_NoEnumsSection_ReturnsError()
        {
            const string source = "@DATA( port = 8080 )";

            var result = MdixEnumCodeGenerator.GenerateFromSource(source);

            _out.WriteLine(result.IsFailure ? result.Error.Message : "unexpectedly succeeded");
            result.IsFailure.Should().BeTrue();
        }

        [Fact]
        public void GenerateFromSource_UnclosedEnumBlock_ReturnsError()
        {
            const string source = "@ENUMS( LogLevel { DEBUG = 0, INFO = 1 )";

            var result = MdixEnumCodeGenerator.GenerateFromSource(source);

            _out.WriteLine(result.IsFailure ? result.Error.Message : "unexpectedly succeeded");
            result.IsFailure.Should().BeTrue();
        }

        [Fact]
        public void GenerateFromSource_UnclosedEnumsSection_ReturnsError()
        {
            const string source = "@ENUMS( LogLevel { DEBUG = 0, INFO = 1 }";

            var result = MdixEnumCodeGenerator.GenerateFromSource(source);

            _out.WriteLine(result.IsFailure ? result.Error.Message : "unexpectedly succeeded");
            result.IsFailure.Should().BeTrue();
        }

        [Fact]
        public void GenerateFromSource_NullSource_ReturnsError()
        {
            MdixEnumCodeGenerator.GenerateFromSource(null!).IsFailure.Should().BeTrue();
        }

        [Fact]
        public void GenerateFromFile_ReadsFromDisk_ProducesMatchingEnum()
        {
            File.WriteAllText(_path, "@ENUMS( Environment { DEV = 1, STAGING = 2, PROD = 3 } )");

            var result = MdixEnumCodeGenerator.GenerateFromFile(_path);

            _out.WriteLine(result.IsSuccess ? result.SuccessResult : result.Error.Message);

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().Contain("public enum Environment");
            result.SuccessResult.Should().Contain("PROD = 3");
        }

        [Fact]
        public void GenerateFromFile_MissingFile_ReturnsIoError()
        {
            var missingPath = Path.Combine(Path.GetTempPath(), $"mdix-does-not-exist-{Guid.NewGuid():N}.mdix");

            var result = MdixEnumCodeGenerator.GenerateFromFile(missingPath);

            _out.WriteLine(result.IsFailure ? result.Error.Message : "unexpectedly succeeded");
            result.IsFailure.Should().BeTrue();
        }

        [Fact]
        public void GenerateFromSource_GeneratedTypeName_MatchesEnumNameForSerializerRoundTrip()
        {
            // The convention MdixSerializer's write path relies on (see
            // ApplyFlat/ApplyTable's `case Enum en:` handling): the C# enum
            // type's own Name must equal the DixScript enum's declared name.
            // A generated enum should satisfy this by construction.
            const string source = "@ENUMS( Environment { DEV = 1, STAGING = 2, PROD = 3 } )";

            var result = MdixEnumCodeGenerator.GenerateFromSource(source, "MyApp.Config");

            result.IsSuccess.Should().BeTrue();
            result.SuccessResult.Should().MatchRegex(@"(?m)^\s*public enum Environment\s*$");
        }
    }
}
