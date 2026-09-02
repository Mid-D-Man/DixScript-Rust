<?php
declare(strict_types=1);

namespace MidManStudio\Mdix;

use MidManStudio\Mdix\Internal\NativeLoader;

/**
 * Watches a single plaintext .mdix file on disk and reloads it through the
 * full loader pipeline only when its modified-time has changed. Mirrors
 * Rust's dixscript::Runtime::HotReloadWatcher directly — a cheap stat()-based
 * poll, not an OS filesystem-event subscription, so it's safe (and
 * inexpensive) to call from a request-loop / scheduled task every tick.
 *
 *   $watcher = new MdixHotReload('config.mdix');
 *   try {
 *       while ($running) {
 *           $fresh = $watcher->checkAndReload();
 *           if ($fresh !== null) {
 *               try {
 *                   applyNewConfig($fresh);
 *               } finally {
 *                   $fresh->close();
 *               }
 *           }
 *           tick();
 *       }
 *   } finally {
 *       $watcher->close();
 *   }
 *
 * Encrypted files are NOT supported. HotReloadWatcher::force_reload()
 * always reloads through the plaintext loader path — this is a limitation
 * of the core Runtime feature itself, not something this binding adds on
 * top.
 *
 * On a reload failure (e.g. the file was saved mid-write and is briefly
 * invalid), the watcher's internal modified-time stamp is not updated, so
 * the next check retries against the same file state rather than silently
 * giving up on that change.
 */
final class MdixHotReload implements \Stringable
{
    /** @var \FFI\CData|null */
    private mixed $handle;
    private bool  $closed = false;

    /** Starts watching $path. Does not read the file yet — the first checkAndReload() always reports a change. */
    public function __construct(string $path)
    {
        $this->handle = NativeLoader::get()->mdix_watcher_new($path);

        if ($this->handle === null) {
            throw new MdixError("Failed to create watcher for '{$path}'", ErrorKind::Native);
        }
    }

    public function close(): void
    {
        if (!$this->closed && $this->handle !== null) {
            NativeLoader::get()->mdix_watcher_free($this->handle);
            $this->handle = null;
            $this->closed = true;
        }
    }

    public function __destruct()
    {
        $this->close();
    }

    /** The path this watcher was constructed with. */
    public function path(): string
    {
        $this->assertOpen();
        $ffi = NativeLoader::get();
        $ptr = $ffi->mdix_watcher_path($this->handle);
        if ($ptr === null) {
            return '';
        }
        $value = \FFI::string($ptr);
        $ffi->mdix_free_string($ptr);
        return $value;
    }

    /** True once a successful reload has happened at least once. */
    public function hasLoaded(): bool
    {
        $this->assertOpen();
        return (bool) NativeLoader::get()->mdix_watcher_has_loaded($this->handle);
    }

    /** Checks whether the file's modified-time differs from the last successful reload, without reloading it. */
    public function hasChanged(): bool
    {
        $this->assertOpen();
        return (bool) NativeLoader::get()->mdix_watcher_has_changed($this->handle);
    }

    /**
     * Reloads only if the file has changed since the last successful reload
     * (or since construction, on the first call). Returns null when the
     * file is unchanged. The caller owns the returned MdixDatabase and must
     * close it.
     */
    public function checkAndReload(): ?MdixDatabase
    {
        $this->assertOpen();
        $handle = NativeLoader::get()->mdix_watcher_check_and_reload($this->handle);
        return $handle === null ? null : MdixDatabase::adopt($handle);
    }

    /** Reloads unconditionally, regardless of whether the file has changed. The caller owns the returned MdixDatabase. */
    public function forceReload(): MdixDatabase
    {
        $this->assertOpen();
        $handle = NativeLoader::get()->mdix_watcher_force_reload($this->handle);
        if ($handle === null) {
            $ffi = NativeLoader::get();
            $ptr = $ffi->mdix_get_last_error();
            $msg = $ptr !== null ? $ptr : 'unknown native error';
            throw MdixError::fromMessage("[mdix:forceReload] {$msg}");
        }
        return MdixDatabase::adopt($handle);
    }

    public function __toString(): string
    {
        if ($this->closed || $this->handle === null) {
            return 'MdixHotReload(closed)';
        }
        return \sprintf('MdixHotReload(%s)', $this->path());
    }

    private function assertOpen(): void
    {
        if ($this->closed || $this->handle === null) {
            throw new MdixError('MdixHotReload has been closed', ErrorKind::Closed);
        }
    }
}
