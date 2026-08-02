using System;
using System.IO;
using System.Threading;
using FluentAssertions;
using MidManStudio.Mdix;
using MidManStudio.Mdix.Core;
using Xunit;
using Xunit.Abstractions;

namespace MidManStudio.Mdix.Core.Tests
{
    // FIX: MdixDatabase's hot-reload feature (EnableHotReload / OnReloaded /
    // OnReloadFailed / DisableHotReload, backed by a real
    // System.IO.FileSystemWatcher -- see the "Hot reload" region in
    // MdixDatabase.cs) had zero test coverage before this file, despite being
    // some of the most concurrency-sensitive code in the package: a debounced
    // reload guard (ReloadDebounceTicks), a SafeHandle
    // DangerousAddRef/DangerousRelease swap racing against Dispose, and an
    // `async void` event handler where an uncaught exception would crash the
    // process rather than just failing a Task.
    //
    // xUnit creates a fresh instance of this class per [Fact], so the
    // constructor/Dispose pair below gives each test its own temp file on
    // disk, cleaned up whether the test passes or throws.
    public class MdixHotReloadTests : IDisposable
    {
        private readonly ITestOutputHelper _out;
        private readonly string _path;

        public MdixHotReloadTests(ITestOutputHelper output)
        {
            _out  = output;
            _path = Path.Combine(Path.GetTempPath(), $"mdix-hotreload-test-{Guid.NewGuid():N}.mdix");
        }

        public void Dispose()
        {
            try { if (File.Exists(_path)) File.Delete(_path); }
            catch { /* best-effort cleanup -- a leaked temp file isn't worth failing the test run over */ }
        }

        // FileSystemWatcher notifications are asynchronous and not
        // instantaneous even on a local disk -- poll instead of a single
        // fixed-delay Thread.Sleep, which would make this flaky under CI load
        // either by checking too early (false failure) or wasting far more
        // time than needed on a fast machine. 5s comfortably exceeds the
        // 500ms ReloadDebounceTicks window (5_000_000 ticks) a single write
        // has to clear.
        private static bool WaitUntil(Func<bool> condition, int timeoutMs = 5000, int pollMs = 25)
        {
            var deadline = DateTime.UtcNow.AddMilliseconds(timeoutMs);
            while (DateTime.UtcNow < deadline)
            {
                if (condition()) return true;
                Thread.Sleep(pollMs);
            }
            return condition();
        }

        [Fact]
        public void EnableHotReload_LoadedViaLoadStr_ThrowsInvalidOperationException()
        {
            // No _sourcePath exists for an in-memory-loaded database -- there is
            // nothing on disk to watch, so this should fail fast and clearly
            // rather than silently doing nothing.
            using var db = Dix.LoadStr("@DATA( n = 1 )").OrThrow();

            var ex = Record.Exception(() => db.EnableHotReload());

            _out.WriteLine($"Exception: {ex?.GetType().Name ?? "none"}");
            ex.Should().BeOfType<InvalidOperationException>();
        }

        [Fact]
        public void EnableHotReload_FileModified_RaisesOnReloaded()
        {
            File.WriteAllText(_path, "@DATA( port = 8080 )");
            using var db = Dix.Load(_path).OrThrow();

            MdixDatabase? reloaded = null;
            MdixError?    failure  = null;
            db.OnReloaded     += newDb => reloaded = newDb;
            db.OnReloadFailed += err   => failure  = err;
            db.EnableHotReload();

            File.WriteAllText(_path, "@DATA( port = 9090 )");

            var fired = WaitUntil(() => reloaded is not null || failure is not null);

            _out.WriteLine(
                $"fired: {fired}, reloaded: {reloaded is not null}, " +
                $"failure: {failure?.Message ?? "none"}");

            fired.Should().BeTrue("OnReloaded or OnReloadFailed should fire after the watched file changes");
            failure.Should().BeNull();
            reloaded.Should().NotBeNull();
            reloaded!.GetInt("port").OrThrow().Should().Be(9090);

            db.DisableHotReload();
            reloaded.Dispose();
        }

        [Fact]
        public void EnableHotReload_ReloadedDatabase_IsIndependentlyUsable()
        {
            File.WriteAllText(_path, "@DATA( value = 1 )");
            using var db = Dix.Load(_path).OrThrow();

            MdixDatabase? reloaded = null;
            db.OnReloaded += newDb => reloaded = newDb;
            db.EnableHotReload();

            File.WriteAllText(_path, "@DATA( value = 2 )");
            WaitUntil(() => reloaded is not null).Should().BeTrue();

            db.DisableHotReload();

            // Confirms the SafeHandle swap inside Reload() produced a live,
            // separately-owned handle -- not an alias of the original that
            // would break once `db` above is disposed at the end of this test.
            reloaded!.GetInt("value").OrThrow().Should().Be(2);
            reloaded.Dispose();
        }

        [Fact]
        public void DisableHotReload_AfterEnable_StopsFurtherReloads()
        {
            File.WriteAllText(_path, "@DATA( n = 1 )");
            using var db = Dix.Load(_path).OrThrow();

            var reloadCount = 0;
            db.OnReloaded += _ => Interlocked.Increment(ref reloadCount);
            db.EnableHotReload();
            db.DisableHotReload();

            File.WriteAllText(_path, "@DATA( n = 2 )");

            // Give a watcher that wasn't actually disabled a fair chance to
            // fire before asserting it didn't -- same timeout as the positive
            // tests above, just asserting the negative outcome here.
            WaitUntil(() => reloadCount > 0, timeoutMs: 1000).Should().BeFalse();
            reloadCount.Should().Be(0);
        }

        [Fact]
        public void EnableHotReload_CalledTwice_IsIdempotent()
        {
            // EnableHotReload guards on `if (_watcher != null) return;` -- the
            // second call should be a silent no-op, not throw and not result
            // in more than one eventual reload for a single file write.
            File.WriteAllText(_path, "@DATA( n = 1 )");
            using var db = Dix.Load(_path).OrThrow();

            var reloadCount = 0;
            db.OnReloaded += _ => Interlocked.Increment(ref reloadCount);

            var ex = Record.Exception(() =>
            {
                db.EnableHotReload();
                db.EnableHotReload();
            });

            _out.WriteLine($"Exception: {ex?.Message ?? "none"}");
            ex.Should().BeNull();

            File.WriteAllText(_path, "@DATA( n = 2 )");
            WaitUntil(() => reloadCount > 0).Should().BeTrue();

            // Past the 500ms debounce window, a second (duplicate-subscription)
            // fire for the same write would have landed by now if one existed.
            Thread.Sleep(700);
            reloadCount.Should().Be(1);

            db.DisableHotReload();
        }

        [Fact]
        public void DisableHotReload_WithoutEnable_DoesNotThrow()
        {
            File.WriteAllText(_path, "@DATA( n = 1 )");
            using var db = Dix.Load(_path).OrThrow();

            var ex = Record.Exception(() => db.DisableHotReload());

            _out.WriteLine($"Exception: {ex?.Message ?? "none"}");
            ex.Should().BeNull();
        }
    }
}
