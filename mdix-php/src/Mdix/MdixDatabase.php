<?php
declare(strict_types=1);

namespace MidManStudio\Mdix;

use MidManStudio\Mdix\Internal\NativeLoader;

/**
 * A loaded, read-only DixScript database.
 *
 * Always call close() when done — it releases native memory.
 * Implements the Stringable interface for debugging.
 *
 *   $db = MdixDatabase::loadStr('@DATA( port = 8080, host = "localhost" )');
 *   try {
 *       $port = $db->getInt('port');
 *       $host = $db->getString('host');
 *   } finally {
 *       $db->close();
 *   }
 *
 * Or use the static factory helpers that return MdixResult for railway style:
 *
 *   $port = MdixDatabase::tryLoadStr($source)
 *       ->andThen(fn($db) => $db->tryGetInt('port'))
 *       ->unwrapOr(8080);
 */
final class MdixDatabase implements \Stringable
{
    /** @var \FFI\CData|null Opaque void* handle from the native library. */
    private mixed $handle;
    private bool  $closed = false;

    private function __construct(mixed $handle)
    {
        $this->handle = $handle;
    }

    /**
     * Wraps a raw handle produced elsewhere (MdixMerge, MdixHotReload, ...)
     * in an owning MdixDatabase. Passing a handle not produced by this
     * library, or one already owned elsewhere, is undefined behavior —
     * same contract as close() on every other handle.
     *
     * @internal
     */
    public static function adopt(mixed $handle): self
    {
        return new self($handle);
    }

    // ── Lifecycle ─────────────────────────────────────────────────────────────

    /**
     * Release native memory. Safe to call multiple times.
     */
    public function close(): void
    {
        if (!$this->closed && $this->handle !== null) {
            NativeLoader::get()->mdix_free($this->handle);
            $this->handle = null;
            $this->closed = true;
        }
    }

    public function isValid(): bool
    {
        return !$this->closed
            && $this->handle !== null
            && (bool) NativeLoader::get()->mdix_is_valid($this->handle);
    }

    public function entryCount(): int
    {
        $this->assertOpen();
        return NativeLoader::get()->mdix_entry_count($this->handle);
    }

    /** DLM compression flag recorded when this data was loaded. */
    public function isCompressed(): bool
    {
        $this->assertOpen();
        return (bool) NativeLoader::get()->mdix_is_compressed($this->handle);
    }

    /** DLM encryption flag recorded when this data was loaded. */
    public function isEncrypted(): bool
    {
        $this->assertOpen();
        return (bool) NativeLoader::get()->mdix_is_encrypted($this->handle);
    }

    /**
     * Runtime version string recorded in the loaded data itself (may differ
     * from the current mdix_ffi build if the file was produced by a
     * different mdix-cli).
     *
     * @throws MdixError on failure.
     */
    public function getLoadedVersion(): string
    {
        $this->assertOpen();
        $ffi = NativeLoader::get();
        $ptr = $ffi->mdix_get_loaded_version($this->handle);

        if ($ptr === null) {
            throw self::nativeError('getLoadedVersion');
        }

        $value = \FFI::string($ptr);
        $ffi->mdix_free_string($ptr);

        return $value;
    }

    /**
     * Reads a key from the loaded @CONFIG section (e.g. "version", "author",
     * "debug_mode" — all @CONFIG values are strings).
     *
     * @throws MdixError if the key isn't set.
     */
    public function getConfigValue(string $key): string
    {
        $this->assertOpen();
        $ffi = NativeLoader::get();
        $ptr = $ffi->mdix_get_config_value($this->handle, $key);

        if ($ptr === null) {
            throw self::nativeError("getConfigValue('{$key}')");
        }

        $value = \FFI::string($ptr);
        $ffi->mdix_free_string($ptr);

        return $value;
    }

    public function tryGetConfigValue(string $key): MdixResult
    {
        try {
            return MdixResult::ok($this->getConfigValue($key));
        } catch (\Throwable $e) {
            return MdixResult::fromThrowable($e);
        }
    }

    // ── Loading — raising ─────────────────────────────────────────────────────

    /**
     * Load a .mdix file from disk.
     *
     * @throws MdixError on parse or IO error.
     */
    public static function load(string $path): self
    {
        if ($path === '') {
            throw new MdixError('path must not be empty', ErrorKind::InvalidPath);
        }

        $ffi    = NativeLoader::get();
        $handle = $ffi->mdix_load($path);

        if ($handle === null) {
            throw self::nativeError("load('{$path}')");
        }

        return new self($handle);
    }

    /**
     * Load .mdix content from a source string.
     *
     * @throws MdixError on parse error.
     */
    public static function loadStr(string $source): self
    {
        if ($source === '') {
            throw new MdixError('source must not be empty', ErrorKind::Parse);
        }

        $ffi    = NativeLoader::get();
        $handle = $ffi->mdix_load_str($source);

        if ($handle === null) {
            throw self::nativeError('loadStr');
        }

        return new self($handle);
    }

    /**
     * Load an encrypted .mdix.enc file using a key file.
     * Pass null for $keyPath to auto-detect next to the .enc file.
     *
     * @throws MdixError on decryption or IO error.
     */
    public static function loadEncrypted(string $encPath, ?string $keyPath = null): self
    {
        if ($encPath === '') {
            throw new MdixError('encPath must not be empty', ErrorKind::InvalidPath);
        }

        $ffi    = NativeLoader::get();
        $handle = $ffi->mdix_load_encrypted($encPath, $keyPath);

        if ($handle === null) {
            throw self::nativeError("loadEncrypted('{$encPath}')");
        }

        return new self($handle);
    }

    /**
     * Load an encrypted .mdix.enc file using a password.
     *
     * @throws MdixError on decryption or IO error.
     */
    public static function loadEncryptedPassword(string $encPath, string $password): self
    {
        if ($encPath === '') {
            throw new MdixError('encPath must not be empty', ErrorKind::InvalidPath);
        }
        if ($password === '') {
            throw new MdixError('password must not be empty', ErrorKind::InvalidPath);
        }

        $ffi    = NativeLoader::get();
        $handle = $ffi->mdix_load_encrypted_password($encPath, $password);

        if ($handle === null) {
            throw self::nativeError("loadEncryptedPassword('{$encPath}')");
        }

        return new self($handle);
    }

    /**
     * Parse a JSON object string into a new database.
     *
     * @throws MdixError on parse error.
     */
    public static function fromJson(string $json): self
    {
        if ($json === '') {
            throw new MdixError('json must not be empty', ErrorKind::Parse);
        }

        $ffi    = NativeLoader::get();
        $handle = $ffi->mdix_from_json($json);

        if ($handle === null) {
            throw self::nativeError('fromJson');
        }

        return new self($handle);
    }

    /**
     * Parse a TOML table string into a new database.
     *
     * @throws MdixError on parse error.
     */
    public static function fromToml(string $toml): self
    {
        if ($toml === '') {
            throw new MdixError('toml must not be empty', ErrorKind::Parse);
        }

        $ffi    = NativeLoader::get();
        $handle = $ffi->mdix_from_toml($toml);

        if ($handle === null) {
            throw self::nativeError('fromToml');
        }

        return new self($handle);
    }

    // ── Loading — railway ─────────────────────────────────────────────────────

    public static function tryLoad(string $path): MdixResult
    {
        try {
            return MdixResult::ok(self::load($path));
        } catch (\Throwable $e) {
            return MdixResult::fromThrowable($e);
        }
    }

    public static function tryLoadStr(string $source): MdixResult
    {
        try {
            return MdixResult::ok(self::loadStr($source));
        } catch (\Throwable $e) {
            return MdixResult::fromThrowable($e);
        }
    }

    public static function tryFromJson(string $json): MdixResult
    {
        try {
            return MdixResult::ok(self::fromJson($json));
        } catch (\Throwable $e) {
            return MdixResult::fromThrowable($e);
        }
    }

    public static function tryFromToml(string $toml): MdixResult
    {
        try {
            return MdixResult::ok(self::fromToml($toml));
        } catch (\Throwable $e) {
            return MdixResult::fromThrowable($e);
        }
    }

    // ── Type inspection ───────────────────────────────────────────────────────

    public function exists(string $path): bool
    {
        if ($this->closed || $this->handle === null) {
            return false;
        }

        return (bool) NativeLoader::get()->mdix_exists($this->handle, $path);
    }

    public function valueTypeAt(string $path): ValueType
    {
        $this->assertOpen();
        $code = NativeLoader::get()->mdix_get_type($this->handle, $path);
        return ValueType::from($code);
    }

    public function arrayLength(string $path): int
    {
        $this->assertOpen();
        $n = NativeLoader::get()->mdix_get_array_length($this->handle, $path);

        if ($n < 0) {
            throw new MdixError(
                "arrayLength: not an array at path '{$path}'",
                ErrorKind::TypeMismatch,
            );
        }

        return $n;
    }

    /**
     * Return direct child key names under $prefix.
     * Pass empty string for top-level keys.
     *
     * @return string[]
     */
    public function keys(string $prefix = ''): array
    {
        $this->assertOpen();
        $ffi   = NativeLoader::get();
        $count = $ffi->new('int32_t');
        $arr   = $ffi->mdix_get_keys($this->handle, $prefix, \FFI::addr($count));

        if ($arr === null || $count->cdata <= 0) {
            return [];
        }

        $result = [];
        $n      = (int) $count->cdata;

        for ($i = 0; $i < $n; $i++) {
            $result[] = \FFI::string($arr[$i]);
        }

        $ffi->mdix_free_string_array($arr, $count->cdata);

        return $result;
    }

    /**
     * Every key in the entire flattened data set (recursive) — not just
     * direct children of a prefix, unlike keys().
     *
     * @return string[]
     */
    public function getAllKeys(): array
    {
        $this->assertOpen();
        $ffi   = NativeLoader::get();
        $count = $ffi->new('int32_t');
        $arr   = $ffi->mdix_get_all_keys($this->handle, \FFI::addr($count));

        if ($arr === null || $count->cdata <= 0) {
            return [];
        }

        $result = [];
        $n      = (int) $count->cdata;

        for ($i = 0; $i < $n; $i++) {
            $result[] = \FFI::string($arr[$i]);
        }

        $ffi->mdix_free_string_array($arr, $count->cdata);

        return $result;
    }

    // ── Typed getters — raising ───────────────────────────────────────────────

    /**
     * @throws MdixError if the path does not exist or is the wrong type.
     */
    public function getString(string $path, ?string $default = null): string
    {
        $this->assertOpen();
        $this->assertPath($path);

        $ffi = NativeLoader::get();
        $ptr = $ffi->mdix_get_string($this->handle, $path);

        if ($ptr === null) {
            if ($default !== null) {
                return $default;
            }
            throw self::nativeError("getString('{$path}')");
        }

        $value = \FFI::string($ptr);
        $ffi->mdix_free_string($ptr);

        return $value;
    }

    /**
     * @throws MdixError if the path does not exist or is the wrong type.
     */
    public function getInt(string $path, ?int $default = null): int
    {
        $this->assertOpen();
        $this->assertPath($path);

        if ($default !== null && !$this->exists($path)) {
            return $default;
        }

        $ffi = NativeLoader::get();
        $ffi->mdix_clear_error();
        $value = (int) $ffi->mdix_get_int($this->handle, $path);

        $err = $ffi->mdix_get_last_error();
        if ($err !== null) {
            throw self::nativeErrorFromPtr($err, "getInt('{$path}')");
        }

        return $value;
    }

    /**
     * Reads a 64-bit integer. Also accepts Int values (widened without loss).
     *
     * @throws MdixError if the path does not exist or is the wrong type.
     */
    public function getLong(string $path, ?int $default = null): int
    {
        $this->assertOpen();
        $this->assertPath($path);

        if ($default !== null && !$this->exists($path)) {
            return $default;
        }

        $ffi = NativeLoader::get();
        $ffi->mdix_clear_error();
        $value = (int) $ffi->mdix_get_long($this->handle, $path);

        $err = $ffi->mdix_get_last_error();
        if ($err !== null) {
            throw self::nativeErrorFromPtr($err, "getLong('{$path}')");
        }

        return $value;
    }

    /**
     * @throws MdixError if the path does not exist or is the wrong type.
     */
    public function getFloat(string $path, ?float $default = null): float
    {
        $this->assertOpen();
        $this->assertPath($path);

        if ($default !== null && !$this->exists($path)) {
            return $default;
        }

        $ffi = NativeLoader::get();
        $ffi->mdix_clear_error();
        $value = (float) $ffi->mdix_get_float($this->handle, $path);

        $err = $ffi->mdix_get_last_error();
        if ($err !== null) {
            throw self::nativeErrorFromPtr($err, "getFloat('{$path}')");
        }

        return $value;
    }

    /**
     * @throws MdixError if the path does not exist or is the wrong type.
     */
    public function getDouble(string $path, ?float $default = null): float
    {
        $this->assertOpen();
        $this->assertPath($path);

        if ($default !== null && !$this->exists($path)) {
            return $default;
        }

        $ffi = NativeLoader::get();
        $ffi->mdix_clear_error();
        $value = (float) $ffi->mdix_get_double($this->handle, $path);

        $err = $ffi->mdix_get_last_error();
        if ($err !== null) {
            throw self::nativeErrorFromPtr($err, "getDouble('{$path}')");
        }

        return $value;
    }

    /**
     * @throws MdixError if the path does not exist or is the wrong type.
     */
    public function getBool(string $path, ?bool $default = null): bool
    {
        $this->assertOpen();
        $this->assertPath($path);

        if ($default !== null && !$this->exists($path)) {
            return $default;
        }

        $ffi = NativeLoader::get();
        $ffi->mdix_clear_error();
        $value = (bool) $ffi->mdix_get_bool($this->handle, $path);

        $err = $ffi->mdix_get_last_error();
        if ($err !== null) {
            throw self::nativeErrorFromPtr($err, "getBool('{$path}')");
        }

        return $value;
    }

    // ── Enum helpers ──────────────────────────────────────────────────────────

    /** Returns the enum type name at $path, e.g. "AIType". */
    public function getEnumName(string $path): string
    {
        $this->assertOpen();
        $this->assertPath($path);

        $ffi = NativeLoader::get();
        $ptr = $ffi->mdix_get_enum_name($this->handle, $path);

        if ($ptr === null) {
            throw self::nativeError("getEnumName('{$path}')");
        }

        $value = \FFI::string($ptr);
        $ffi->mdix_free_string($ptr);

        return $value;
    }

    /** Returns the enum field name at $path, e.g. "BOSS". */
    public function getEnumField(string $path): string
    {
        $this->assertOpen();
        $this->assertPath($path);

        $ffi = NativeLoader::get();
        $ptr = $ffi->mdix_get_enum_field($this->handle, $path);

        if ($ptr === null) {
            throw self::nativeError("getEnumField('{$path}')");
        }

        $value = \FFI::string($ptr);
        $ffi->mdix_free_string($ptr);

        return $value;
    }

    /** Returns the resolved integer value of an enum at $path. */
    public function getEnumValue(string $path): int
    {
        return $this->getInt($path);
    }

    // ── JSON escape hatch ─────────────────────────────────────────────────────

    /**
     * Serialise the value at $path to a JSON string.
     * Useful for Blob, Regex, Tuple and nested structures.
     *
     * @throws MdixError if the path does not exist.
     */
    public function getJson(string $path): string
    {
        $this->assertOpen();
        $this->assertPath($path);

        $ffi = NativeLoader::get();
        $ptr = $ffi->mdix_get_json($this->handle, $path);

        if ($ptr === null) {
            throw self::nativeError("getJson('{$path}')");
        }

        $value = \FFI::string($ptr);
        $ffi->mdix_free_string($ptr);

        return $value;
    }

    // ── Typed getters — railway ───────────────────────────────────────────────

    public function tryGetString(string $path): MdixResult
    {
        try {
            return MdixResult::ok($this->getString($path));
        } catch (\Throwable $e) {
            return MdixResult::fromThrowable($e);
        }
    }

    public function tryGetInt(string $path): MdixResult
    {
        try {
            return MdixResult::ok($this->getInt($path));
        } catch (\Throwable $e) {
            return MdixResult::fromThrowable($e);
        }
    }

    public function tryGetLong(string $path): MdixResult
    {
        try {
            return MdixResult::ok($this->getLong($path));
        } catch (\Throwable $e) {
            return MdixResult::fromThrowable($e);
        }
    }

    public function tryGetFloat(string $path): MdixResult
    {
        try {
            return MdixResult::ok($this->getFloat($path));
        } catch (\Throwable $e) {
            return MdixResult::fromThrowable($e);
        }
    }

    public function tryGetDouble(string $path): MdixResult
    {
        try {
            return MdixResult::ok($this->getDouble($path));
        } catch (\Throwable $e) {
            return MdixResult::fromThrowable($e);
        }
    }

    public function tryGetBool(string $path): MdixResult
    {
        try {
            return MdixResult::ok($this->getBool($path));
        } catch (\Throwable $e) {
            return MdixResult::fromThrowable($e);
        }
    }

    public function tryGetJson(string $path): MdixResult
    {
        try {
            return MdixResult::ok($this->getJson($path));
        } catch (\Throwable $e) {
            return MdixResult::fromThrowable($e);
        }
    }

    // ── Query ────────────────────────────────────────────────────────────────

    /**
     * Starts a chainable MdixQuery over the array (or single value) at $path.
     * Equivalent to Rust's data.query(path).
     *
     *   $bossNames = $db->query('enemies')
     *       ->where(fn($e) => ($e['aiType'] ?? null) === 'BOSS')
     *       ->select(fn($e) => $e['name']);
     *
     * @throws MdixError if $path is not found.
     */
    public function query(string $path): MdixQuery
    {
        $json  = $this->getJson($path);
        $value = \json_decode($json, associative: true, flags: \JSON_THROW_ON_ERROR);

        return new MdixQuery(\is_array($value) && \array_is_list($value) ? $value : [$value]);
    }

    /**
     * Starts a chainable MdixQuery over every value matching the
     * whole-segment glob $pattern (e.g. "levels.*.enemies") — sibling
     * paths sharing structure, gathered natively via
     * DixData::select_many. Equivalent to Rust's data.query_many(pattern).
     */
    public function queryMany(string $pattern): MdixQuery
    {
        $this->assertOpen();
        $this->assertPath($pattern);

        $ffi = NativeLoader::get();
        $ptr = $ffi->mdix_select_many_as_json($this->handle, $pattern);

        if ($ptr === null) {
            throw self::nativeError("queryMany('{$pattern}')");
        }

        $json = \FFI::string($ptr);
        $ffi->mdix_free_string($ptr);

        $value = \json_decode($json, associative: true, flags: \JSON_THROW_ON_ERROR);

        return new MdixQuery(\is_array($value) ? $value : []);
    }

    // ── Export ────────────────────────────────────────────────────────────────

    /**
     * Export all entries as a JSON string.
     *
     * @throws MdixError on serialisation failure.
     */
    public function toJson(bool $indented = true): string
    {
        $this->assertOpen();

        $ffi = NativeLoader::get();
        $ptr = $ffi->mdix_to_json($this->handle, $indented);

        if ($ptr === null) {
            throw self::nativeError('toJson');
        }

        $value = \FFI::string($ptr);
        $ffi->mdix_free_string($ptr);

        return $value;
    }

    /**
     * Re-serialise back to .mdix text format.
     *
     * @throws MdixError on serialisation failure.
     */
    public function toMdix(FormatMode $mode = FormatMode::Default): string
    {
        $this->assertOpen();

        $ffi = NativeLoader::get();
        $ptr = $ffi->mdix_to_mdix($this->handle, $mode->value);

        if ($ptr === null) {
            throw self::nativeError('toMdix');
        }

        $value = \FFI::string($ptr);
        $ffi->mdix_free_string($ptr);

        return $value;
    }

    /**
     * Export all entries as a TOML string.
     *
     * @throws MdixError on serialisation failure.
     */
    public function toToml(): string
    {
        $this->assertOpen();

        $ffi = NativeLoader::get();
        $ptr = $ffi->mdix_to_toml($this->handle);

        if ($ptr === null) {
            throw self::nativeError('toToml');
        }

        $value = \FFI::string($ptr);
        $ffi->mdix_free_string($ptr);

        return $value;
    }

    // ── Dunder ────────────────────────────────────────────────────────────────

    public function __toString(): string
    {
        if ($this->closed || $this->handle === null) {
            return 'MdixDatabase(freed)';
        }

        return \sprintf('MdixDatabase(entries=%d)', $this->entryCount());
    }

    public function __destruct()
    {
        $this->close();
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /** @internal Used by MdixConverter */
    public function rawHandle(): mixed
    {
        $this->assertOpen();
        return $this->handle;
    }

    private function assertOpen(): void
    {
        if ($this->closed || $this->handle === null) {
            throw new MdixError('MdixDatabase has been closed', ErrorKind::Closed);
        }
    }

    private function assertPath(string $path): void
    {
        if ($path === '') {
            throw new MdixError('path must not be empty', ErrorKind::InvalidPath);
        }
    }

    private static function nativeError(string $context): MdixError
    {
        $ffi = NativeLoader::get();
        $ptr = $ffi->mdix_get_last_error();
        $msg = $ptr !== null ? $ptr : 'unknown native error';
        return MdixError::fromMessage("[mdix:{$context}] {$msg}");
    }

    private static function nativeErrorFromPtr(mixed $ptr, string $context): MdixError
    {
        // FIX: every call site passes $err from $ffi->mdix_get_last_error()
        // directly. mdix_get_last_error() returns `const char*`, and PHP's
        // FFI auto-converts a *const* char* return value straight to a
        // native PHP string (or null) at the call boundary -- unlike a
        // plain (non-const) char* return, which stays a FFI\CData object
        // requiring an explicit FFI::string() to read. Calling FFI::string()
        // on the already-a-string result threw "must be of type FFI\CData,
        // string given" on every single getInt/getLong/getFloat/getDouble/
        // getBool native-error path -- meaning any real native error there
        // crashed with an unrelated TypeError instead of raising a useful
        // MdixError. $ptr is normalized here so this helper still works
        // correctly if ever called with a genuine CData in the future.
        $msg = $ptr instanceof \FFI\CData ? \FFI::string($ptr) : (string) $ptr;
        return MdixError::fromMessage("[mdix:{$context}] {$msg}");
    }
}
