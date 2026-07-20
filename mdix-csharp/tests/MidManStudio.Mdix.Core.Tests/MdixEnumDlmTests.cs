using System;
using System.IO;
using FluentAssertions;
using MidManStudio.Mdix.Core;
using Xunit;

namespace MidManStudio.Mdix.Core.Tests
{
    /// <summary>
    /// Enum-only @DATA (no strings/ints/other primitives at all, and zero
    /// @QUICKFUNCS anywhere) round-tripped through DLM compression alone,
    /// encryption alone, and both together.
    ///
    /// MdixEnumMixedDataTests.cs covers enum fields sitting alongside
    /// ordinary sibling data via LoadStr -- direct source parsing, no DLM
    /// at all. This file closes the gap that left: this project has zero
    /// DLM coverage of any kind before this, let alone the specific
    /// "enum(s) are the *only* @DATA content" shape that let a real bug
    /// through in dixscript's core (see the header comment in
    /// mdix_files/tests/dlm/11_enum_only_gzip.mdix for the full mechanics
    /// of why "has enums, has nothing else" is the shape that mattered --
    /// mdix-wasm's dlm_compression.rs found and fixed it there first; this
    /// mirrors that coverage on the C# side against the same underlying
    /// dixscript pipeline via LoadEncrypted).
    ///
    /// Unlike mdix-wasm (which has compile_with_dlm and can build its own
    /// fixtures in-memory at test time), this binding's FFI surface is
    /// load-only -- MdixDatabase.cs never wraps a "compile" entry point,
    /// only LoadEncrypted / LoadEncryptedPassword / LoadEncryptedBytes.
    /// So these fixtures are pre-compiled binaries
    /// (fixtures/enum_dlm/*.mdix.enc + *.mdix.key), generated from the
    /// source .mdix files in mdix_files/tests/dlm/ by
    /// scripts/generate_enum_dlm_fixtures.sh, and checked in rather than
    /// produced at test time. Re-run that script after any change to
    /// those source files, or after any fix that touches DLM/enum
    /// resolution, so these binaries stay current.
    /// </summary>
    public class MdixEnumDlmTests
    {
        private static string FixturePath(string fileName) =>
            Path.Combine(AppContext.BaseDirectory, "fixtures", "enum_dlm", fileName);

        /// <summary>
        /// Shared assertions for all three fixtures -- they're compiled from
        /// sources that differ only in their @DLM module list (see
        /// mdix_files/tests/dlm/11..13_enum_only_*.mdix), so the resolved
        /// @DATA content is identical in every case. If any of these three
        /// tests fails while the others pass, the failure is specific to
        /// that DLM module combination, not to enum resolution in general.
        /// </summary>
        private static void AssertEnumOnlyDataResolvesCorrectly(MdixDatabase db)
        {
            db.GetEnumName("status").OrThrow().Should().Be("Status");
            db.GetEnumField("status").OrThrow().Should().Be("PENDING");
            // PENDING is declared as 2. A silent enum-table lookup-miss
            // falls back to 0, which happens to be a different,
            // valid-looking variant (ACTIVE) -- this is the assertion that
            // would actually catch that.
            db.GetEnumValue("status").OrThrow().Should().Be(2);

            db.GetEnumName("role").OrThrow().Should().Be("Role");
            db.GetEnumField("role").OrThrow().Should().Be("EDITOR");
            db.GetEnumValue("role").OrThrow().Should().Be(1);

            // Nested inside a GroupArray -- the exact spot nested-path
            // resolution has broken before (mdix-scaffold GroupArray
            // regression), now combined with the DLM round trip too.
            db.GetEnumField("assignments[0].role").OrThrow().Should().Be("ADMIN");
            db.GetEnumValue("assignments[0].role").OrThrow().Should().Be(0);
            db.GetEnumField("assignments[1].role").OrThrow().Should().Be("VIEWER");
            db.GetEnumValue("assignments[1].role").OrThrow().Should().Be(2);
        }

        [Fact]
        public void EnumOnlyData_CompressionAlone_RoundTripsCorrectly()
        {
            var encPath = FixturePath("11_enum_only_gzip.mdix.enc");
            var keyPath = FixturePath("11_enum_only_gzip.mdix.key");
            File.Exists(encPath).Should().BeTrue(
                $"fixture should exist at {encPath} -- run scripts/generate_enum_dlm_fixtures.sh if missing");

            using var db = Dix.LoadEncrypted(encPath, keyPath).OrThrow();
            AssertEnumOnlyDataResolvesCorrectly(db);
        }

        [Fact]
        public void EnumOnlyData_EncryptionAlone_RoundTripsCorrectly()
        {
            var encPath = FixturePath("12_enum_only_aes256.mdix.enc");
            var keyPath = FixturePath("12_enum_only_aes256.mdix.key");
            File.Exists(encPath).Should().BeTrue(
                $"fixture should exist at {encPath} -- run scripts/generate_enum_dlm_fixtures.sh if missing");

            using var db = Dix.LoadEncrypted(encPath, keyPath).OrThrow();
            AssertEnumOnlyDataResolvesCorrectly(db);
        }

        [Fact]
        public void EnumOnlyData_CompressionAndEncryption_RoundTripCorrectly()
        {
            var encPath = FixturePath("13_enum_only_gzip_aes256.mdix.enc");
            var keyPath = FixturePath("13_enum_only_gzip_aes256.mdix.key");
            File.Exists(encPath).Should().BeTrue(
                $"fixture should exist at {encPath} -- run scripts/generate_enum_dlm_fixtures.sh if missing");

            using var db = Dix.LoadEncrypted(encPath, keyPath).OrThrow();
            AssertEnumOnlyDataResolvesCorrectly(db);
        }

        [Fact]
        public void EnumOnlyData_AutoDetectedKeyFile_AlsoRoundTripsCorrectly()
        {
            // Same as EnumOnlyData_CompressionAndEncryption_RoundTripCorrectly
            // but omitting keyPath -- LoadEncrypted(encPath, null) should
            // auto-detect the sibling .mdix.key file the same way `mdix
            // decrypt` does without --key.
            var encPath = FixturePath("13_enum_only_gzip_aes256.mdix.enc");
            File.Exists(encPath).Should().BeTrue(
                $"fixture should exist at {encPath} -- run scripts/generate_enum_dlm_fixtures.sh if missing");

            using var db = Dix.LoadEncrypted(encPath).OrThrow();
            AssertEnumOnlyDataResolvesCorrectly(db);
        }
    }
}
