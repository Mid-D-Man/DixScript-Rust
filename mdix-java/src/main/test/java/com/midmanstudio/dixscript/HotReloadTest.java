package com.midmanstudio.dixscript;

import org.junit.jupiter.api.*;
import org.junit.jupiter.api.io.TempDir;
import static org.assertj.core.api.Assertions.*;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.FileTime;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Instant;

/**
 * Integration tests for {@link HotReload}.
 * Requires the native lib to be on java.library.path (set by build.gradle.kts).
 * <p>
 * Modified-time changes are set explicitly via {@link Files#setLastModifiedTime} rather
 * than relying on real-time sleeps between writes — several filesystems only offer
 * one-second mtime granularity, which would make a sleep-based test both slow and flaky.
 */
class HotReloadTest {

    private static void write(Path path, String content, Instant mtime) throws IOException {
        Files.write(path, content.getBytes(StandardCharsets.UTF_8));
        Files.setLastModifiedTime(path, FileTime.from(mtime));
    }

    @Test void hasChanged_true_beforeFirstReload(@TempDir Path tmp) throws IOException {
        Path file = tmp.resolve("config.mdix");
        write(file, "@DATA( port = 8080 )", Instant.now());

        try (HotReload watcher = new HotReload(file.toString())) {
            assertThat(watcher.hasLoaded()).isFalse();
            assertThat(watcher.hasChanged()).isTrue();
        }
    }

    @Test void checkAndReload_firstCall_reloadsAndReturnsData(@TempDir Path tmp) throws IOException {
        Path file = tmp.resolve("config.mdix");
        write(file, "@DATA( port = 8080 )", Instant.now());

        try (HotReload watcher = new HotReload(file.toString())) {
            var result = watcher.checkAndReload();
            assertThat(result).isPresent();
            try (Database db = result.get()) {
                assertThat(db.getInt("port")).isEqualTo(8080);
            }
            assertThat(watcher.hasLoaded()).isTrue();
        }
    }

    @Test void checkAndReload_noChange_returnsEmpty(@TempDir Path tmp) throws IOException {
        Path file = tmp.resolve("config.mdix");
        Instant t0 = Instant.now();
        write(file, "@DATA( port = 8080 )", t0);

        try (HotReload watcher = new HotReload(file.toString())) {
            watcher.checkAndReload().ifPresent(Database::close);
            assertThat(watcher.hasChanged()).isFalse();
            assertThat(watcher.checkAndReload()).isEmpty();
        }
    }

    @Test void checkAndReload_afterModification_reloadsAgain(@TempDir Path tmp) throws IOException {
        Path file = tmp.resolve("config.mdix");
        Instant t0 = Instant.now();
        write(file, "@DATA( port = 8080 )", t0);

        try (HotReload watcher = new HotReload(file.toString())) {
            watcher.checkAndReload().ifPresent(Database::close);

            write(file, "@DATA( port = 9090 )", t0.plusSeconds(5));

            var result = watcher.checkAndReload();
            assertThat(result).isPresent();
            try (Database db = result.get()) {
                assertThat(db.getInt("port")).isEqualTo(9090);
            }
        }
    }

    @Test void forceReload_reloadsRegardlessOfChange(@TempDir Path tmp) throws IOException {
        Path file = tmp.resolve("config.mdix");
        write(file, "@DATA( port = 8080 )", Instant.now());

        try (HotReload watcher = new HotReload(file.toString())) {
            watcher.checkAndReload().ifPresent(Database::close);
            try (Database db = watcher.forceReload()) {
                assertThat(db.getInt("port")).isEqualTo(8080);
            }
        }
    }

    @Test void path_returnsConstructorPath(@TempDir Path tmp) throws IOException {
        Path file = tmp.resolve("config.mdix");
        write(file, "@DATA( port = 8080 )", Instant.now());

        try (HotReload watcher = new HotReload(file.toString())) {
            assertThat(watcher.path()).isEqualTo(file.toString());
        }
    }

    @Test void malformedFile_reloadFails_andRetriesOnNextCall(@TempDir Path tmp) throws IOException {
        Path file = tmp.resolve("config.mdix");
        Instant t0 = Instant.now();
        write(file, "@@@INVALID$$$", t0);

        try (HotReload watcher = new HotReload(file.toString())) {
            assertThatThrownBy(watcher::checkAndReload).isInstanceOf(MdixException.class);
            // Failure must not have consumed the "changed" state — a fix-and-retry should still see a change.
            write(file, "@DATA( port = 1 )", t0.plusSeconds(5));
            assertThat(watcher.checkAndReload()).isPresent();
        }
    }

    @Test void missingFile_hasChanged_throws(@TempDir Path tmp) {
        Path file = tmp.resolve("does-not-exist.mdix");
        try (HotReload watcher = new HotReload(file.toString())) {
            assertThatThrownBy(watcher::hasChanged).isInstanceOf(MdixException.class);
        }
    }

    @Test void closedWatcher_throwsOnFurtherUse(@TempDir Path tmp) throws IOException {
        Path file = tmp.resolve("config.mdix");
        write(file, "@DATA( port = 8080 )", Instant.now());

        HotReload watcher = new HotReload(file.toString());
        watcher.close();
        assertThatThrownBy(watcher::hasChanged).isInstanceOf(MdixException.class);
    }

    @Test void close_isIdempotent(@TempDir Path tmp) throws IOException {
        Path file = tmp.resolve("config.mdix");
        write(file, "@DATA( port = 8080 )", Instant.now());

        HotReload watcher = new HotReload(file.toString());
        assertThatCode(() -> { watcher.close(); watcher.close(); }).doesNotThrowAnyException();
    }
}
