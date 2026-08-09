// HotReload.java
package com.midmanstudio.dixscript;

import com.midmanstudio.dixscript.internal.MdixNative;

import java.io.Closeable;
import java.util.Optional;

/**
 * Watches a single plaintext {@code .mdix} file on disk and reloads it through the full
 * loader pipeline only when its modified-time has changed. Mirrors Rust's
 * {@code dixscript::Runtime::HotReloadWatcher} directly — a cheap {@code stat()}-based poll,
 * not an OS filesystem-event subscription, so it's safe (and inexpensive) to call from a
 * game loop / scheduled task every tick.
 * <pre>{@code
 * try (HotReload watcher = new HotReload("config.mdix")) {
 *     while (running) {
 *         watcher.checkAndReload().ifPresent(freshConfig -> {
 *             try (freshConfig) {
 *                 applyNewConfig(freshConfig);
 *             }
 *         });
 *         tick();
 *     }
 * }
 * }</pre>
 * <p>
 * <b>Encrypted files are not supported.</b> {@code HotReloadWatcher::force_reload()} always
 * reloads through the plaintext loader path — this is a limitation of the core Runtime
 * feature itself, not something this binding adds on top.
 * <p>
 * On a reload failure (e.g. the file was saved mid-write and is briefly invalid), the
 * watcher's internal modified-time stamp is <em>not</em> updated, so the next check retries
 * against the same file state rather than silently giving up on that change.
 */
public final class HotReload implements Closeable {

    private volatile long handle;
    private volatile boolean closed = false;

    /** Starts watching {@code path}. Does not read the file yet — the first {@link #checkAndReload} always reports a change. */
    public HotReload(String path) {
        this.handle = MdixNative.watcherNew(path);
    }

    @Override
    public synchronized void close() {
        if (!closed) {
            MdixNative.watcherFree(handle);
            closed = true;
            handle = 0;
        }
    }

    private void checkOpen() {
        if (closed) throw new MdixException(MdixException.Kind.CLOSED, "HotReload: already closed");
    }

    /** The path this watcher was constructed with. */
    public String path() {
        checkOpen();
        return MdixNative.watcherPath(handle);
    }

    /** {@code true} once a successful reload has happened at least once. */
    public boolean hasLoaded() {
        checkOpen();
        return MdixNative.watcherHasLoaded(handle);
    }

    /** Checks whether the file's modified-time differs from the last successful reload, without reloading it. */
    public boolean hasChanged() {
        checkOpen();
        return MdixNative.watcherHasChanged(handle);
    }

    /**
     * Reloads only if the file has changed since the last successful reload (or since
     * construction, on the first call). Returns {@link Optional#empty()} when the file is
     * unchanged. The caller owns the returned {@link Database} and must close it.
     */
    public Optional<Database> checkAndReload() {
        checkOpen();
        long result = MdixNative.watcherCheckAndReload(handle);
        return result == 0 ? Optional.empty() : Optional.of(new Database(result));
    }

    /** Reloads unconditionally, regardless of whether the file has changed. The caller owns the returned {@link Database}. */
    public Database forceReload() {
        checkOpen();
        long result = MdixNative.watcherForceReload(handle);
        if (result == 0) throw new MdixException("HotReload.forceReload: native call returned no handle");
        return new Database(result);
    }
}
