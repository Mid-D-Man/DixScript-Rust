<?php
declare(strict_types=1);

namespace MidManStudio\Mdix;

use MidManStudio\Mdix\Internal\NativeLoader;

/**
 * Builds a .mdix database programmatically by setting key-value pairs.
 *
 * Always call close() when done — or use PHP 8.1+ object destructors which
 * call close() automatically when the builder goes out of scope.
 *
 *   $builder = new MdixBuilder();
 *   try {
 *       $builder->setString('profile.name', 'player1')
 *               ->setInt('profile.level', 42)
 *               ->setBool('profile.active', true);
 *       $builder->saveToFile('profile.mdix');
 *   } finally {
 *       $builder->close();
 *   }
 */
final class MdixBuilder implements \Stringable
{
    /** @var \FFI\CData|null */
    private mixed $handle;
    private bool  $closed = false;

    public function __construct()
    {
        $this->handle = NativeLoader::get()->mdix_builder_new();

        if ($this->handle === null) {
            throw new MdixError(
                'Failed to create native builder handle',
                ErrorKind::Native,
            );
        }
    }

    // ── Lifecycle ─────────────────────────────────────────────────────────────

    public function close(): void
    {
        if (!$this->closed && $this->handle !== null) {
            NativeLoader::get()->mdix_builder_free($this->handle);
            $this->handle = null;
            $this->closed = true;
        }
    }

    public function entryCount(): int
    {
        $this->assertOpen();
        return (int) NativeLoader::get()->mdix_builder_entry_count($this->handle);
    }

    /**
     * Remove all entries without freeing the builder.
     *
     * @throws MdixError on failure.
     */
    public function clear(): self
    {
        $this->assertOpen();

        if (!(bool) NativeLoader::get()->mdix_builder_clear($this->handle)) {
            throw new MdixError('clear() failed', ErrorKind::Native);
        }

        return $this;
    }

    // ── Write ─────────────────────────────────────────────────────────────────

    /**
     * @throws MdixError on invalid path or native failure.
     */
    public function setString(string $path, string $value): self
    {
        $this->assertOpen();
        $this->assertPath($path);

        if (!(bool) NativeLoader::get()->mdix_builder_set_string(
            $this->handle, $path, $value
        )) {
            throw new MdixError("setString('{$path}') failed", ErrorKind::Native);
        }

        return $this;
    }

    public function setInt(string $path, int $value): self
    {
        $this->assertOpen();
        $this->assertPath($path);

        if (!(bool) NativeLoader::get()->mdix_builder_set_int(
            $this->handle, $path, $value
        )) {
            throw new MdixError("setInt('{$path}') failed", ErrorKind::Native);
        }

        return $this;
    }

    public function setFloat(string $path, float $value): self
    {
        $this->assertOpen();
        $this->assertPath($path);

        if (!(bool) NativeLoader::get()->mdix_builder_set_float(
            $this->handle, $path, $value
        )) {
            throw new MdixError("setFloat('{$path}') failed", ErrorKind::Native);
        }

        return $this;
    }

    public function setDouble(string $path, float $value): self
    {
        $this->assertOpen();
        $this->assertPath($path);

        if (!(bool) NativeLoader::get()->mdix_builder_set_double(
            $this->handle, $path, $value
        )) {
            throw new MdixError("setDouble('{$path}') failed", ErrorKind::Native);
        }

        return $this;
    }

    public function setBool(string $path, bool $value): self
    {
        $this->assertOpen();
        $this->assertPath($path);

        if (!(bool) NativeLoader::get()->mdix_builder_set_bool(
            $this->handle, $path, $value
        )) {
            throw new MdixError("setBool('{$path}') failed", ErrorKind::Native);
        }

        return $this;
    }

    /**
     * Store a \DateTimeInterface value as a YYYY-MM-DD date string.
     */
    public function setDate(string $path, \DateTimeInterface $value): self
    {
        return $this->setString($path, $value->format('Y-m-d'));
    }

    /**
     * Store a \DateTimeInterface value as an ISO 8601 timestamp string.
     */
    public function setTimestamp(string $path, \DateTimeInterface $value): self
    {
        return $this->setString($path, $value->format(\DateTimeInterface::ATOM));
    }

    /**
     * Remove a key from the builder.
     *
     * @return bool true if the key existed and was removed.
     */
    public function remove(string $path): bool
    {
        $this->assertOpen();
        $this->assertPath($path);

        return (bool) NativeLoader::get()->mdix_builder_remove($this->handle, $path);
    }

    // ── Read-back ─────────────────────────────────────────────────────────────

    public function hasKey(string $path): bool
    {
        if ($this->closed || $this->handle === null || $path === '') {
            return false;
        }

        return (bool) NativeLoader::get()->mdix_builder_has_key($this->handle, $path);
    }

    /** @throws MdixError if the key does not exist or is not a string. */
    public function getString(string $path): string
    {
        $this->assertOpen();
        $this->assertPath($path);

        $ffi = NativeLoader::get();
        $ptr = $ffi->mdix_builder_get_string($this->handle, $path);

        if ($ptr === null) {
            throw MdixError::fromMessage(
                "[mdix:builder_getString('{$path}')] key not found or wrong type"
            );
        }

        $value = \FFI::string($ptr);
        $ffi->mdix_free_string($ptr);

        return $value;
    }

    /** @throws MdixError if the key does not exist or is not numeric. */
    public function getInt(string $path): int
    {
        $this->assertOpen();
        $this->assertPath($path);

        $ffi = NativeLoader::get();
        $ffi->mdix_clear_error();
        $value = (int) $ffi->mdix_builder_get_int($this->handle, $path);

        $err = $ffi->mdix_get_last_error();
        if ($err !== null) {
            throw MdixError::fromMessage(\FFI::string($err));
        }

        return $value;
    }

    /** @throws MdixError if the key does not exist or is not numeric. */
    public function getFloat(string $path): float
    {
        $this->assertOpen();
        $this->assertPath($path);

        $ffi = NativeLoader::get();
        $ffi->mdix_clear_error();
        $value = (float) $ffi->mdix_builder_get_float($this->handle, $path);

        $err = $ffi->mdix_get_last_error();
        if ($err !== null) {
            throw MdixError::fromMessage(\FFI::string($err));
        }

        return $value;
    }

    /** @throws MdixError if the key does not exist or is not numeric. */
    public function getDouble(string $path): float
    {
        $this->assertOpen();
        $this->assertPath($path);

        $ffi = NativeLoader::get();
        $ffi->mdix_clear_error();
        $value = (float) $ffi->mdix_builder_get_double($this->handle, $path);

        $err = $ffi->mdix_get_last_error();
        if ($err !== null) {
            throw MdixError::fromMessage(\FFI::string($err));
        }

        return $value;
    }

    /** @throws MdixError if the key does not exist or is not a bool. */
    public function getBool(string $path): bool
    {
        $this->assertOpen();
        $this->assertPath($path);

        $ffi = NativeLoader::get();
        $ffi->mdix_clear_error();
        $value = (bool) $ffi->mdix_builder_get_bool($this->handle, $path);

        $err = $ffi->mdix_get_last_error();
        if ($err !== null) {
            throw MdixError::fromMessage(\FFI::string($err));
        }

        return $value;
    }

    // ── Persistence ───────────────────────────────────────────────────────────

    /**
     * Save the builder contents to a .mdix file on disk.
     * Intermediate directories are created automatically by the native lib.
     *
     * @throws MdixError on IO failure.
     */
    public function saveToFile(string $path): void
    {
        $this->assertOpen();
        $this->assertPath($path);

        if (!(bool) NativeLoader::get()->mdix_builder_save($this->handle, $path)) {
            $ffi = NativeLoader::get();
            $ptr = $ffi->mdix_get_last_error();
            $msg = $ptr !== null ? \FFI::string($ptr) : 'unknown IO error';
            throw new MdixError($msg, ErrorKind::Io);
        }
    }

    /**
     * Serialise the builder contents to a .mdix format string.
     *
     * @throws MdixError on serialisation failure.
     */
    public function toMdixString(): string
    {
        $this->assertOpen();

        $ffi = NativeLoader::get();
        $ptr = $ffi->mdix_builder_to_string($this->handle);

        if ($ptr === null) {
            throw new MdixError('toMdixString() failed', ErrorKind::Native);
        }

        $value = \FFI::string($ptr);
        $ffi->mdix_free_string($ptr);

        return $value;
    }

    /**
     * Serialise and immediately load the builder into a new MdixDatabase.
     *
     * The caller is responsible for closing the returned database.
     *
     * @throws MdixError on parse failure.
     */
    public function toDatabase(): MdixDatabase
    {
        return MdixDatabase::loadStr($this->toMdixString());
    }

    /**
     * Railway variant of toDatabase() — never throws.
     */
    public function tryToDatabase(): MdixResult
    {
        try {
            return MdixResult::ok($this->toDatabase());
        } catch (\Throwable $e) {
            return MdixResult::fromThrowable($e);
        }
    }

    // ── Dunder ────────────────────────────────────────────────────────────────

    public function __toString(): string
    {
        if ($this->closed || $this->handle === null) {
            return 'MdixBuilder(closed)';
        }

        return \sprintf('MdixBuilder(entries=%d)', $this->entryCount());
    }

    public function __destruct()
    {
        $this->close();
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    private function assertOpen(): void
    {
        if ($this->closed || $this->handle === null) {
            throw new MdixError('MdixBuilder has been closed', ErrorKind::Closed);
        }
    }

    private function assertPath(string $path): void
    {
        if ($path === '') {
            throw new MdixError('path must not be empty', ErrorKind::InvalidPath);
        }
    }
}
