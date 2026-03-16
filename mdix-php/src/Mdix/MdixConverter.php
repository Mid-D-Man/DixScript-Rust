<?php
declare(strict_types=1);

namespace MidManStudio\Mdix;

use MidManStudio\Mdix\Internal\NativeLoader;

/**
 * Format conversion utilities: JSON ↔ MdixDatabase, TOML ↔ MdixDatabase,
 * .mdix source formatting and minification.
 *
 * All methods are static — no instantiation required.
 *
 *   $json = MdixConverter::toJson($db, indented: true);
 *   $db2  = MdixConverter::fromJson($json);
 *   $db2->close();
 *
 *   $formatted = MdixConverter::formatSource($rawMdix, FormatMode::Pretty);
 */
final class MdixConverter
{
    // ── Export ────────────────────────────────────────────────────────────────

    /**
     * Export all entries in $db as a JSON string.
     *
     * @throws MdixError on serialisation failure.
     */
    public static function toJson(MdixDatabase $db, bool $indented = true): string
    {
        return $db->toJson($indented);
    }

    /**
     * Re-serialise $db to .mdix text format.
     *
     * @throws MdixError on serialisation failure.
     */
    public static function toMdix(
        MdixDatabase $db,
        FormatMode $mode = FormatMode::Default,
    ): string {
        return $db->toMdix($mode);
    }

    /**
     * Export all entries in $db as a TOML string.
     *
     * @throws MdixError on serialisation failure.
     */
    public static function toToml(MdixDatabase $db): string
    {
        return $db->toToml();
    }

    // ── Import ────────────────────────────────────────────────────────────────

    /**
     * Parse a JSON object string into a new MdixDatabase.
     * The caller must close the returned database.
     *
     * @throws MdixError on parse failure.
     */
    public static function fromJson(string $json): MdixDatabase
    {
        return MdixDatabase::fromJson($json);
    }

    /**
     * Parse a TOML table string into a new MdixDatabase.
     * The caller must close the returned database.
     *
     * @throws MdixError on parse failure.
     */
    public static function fromToml(string $toml): MdixDatabase
    {
        return MdixDatabase::fromToml($toml);
    }

    // ── Source text formatting ────────────────────────────────────────────────

    /**
     * Format raw .mdix source text according to $mode.
     *
     * @throws MdixError if formatting fails.
     */
    public static function formatSource(
        string $source,
        FormatMode $mode = FormatMode::Default,
    ): string {
        if ($source === '') {
            throw new MdixError('source must not be empty', ErrorKind::InvalidPath);
        }

        $ffi = NativeLoader::get();
        $ptr = $ffi->mdix_format_source($source, $mode->value);

        if ($ptr === null) {
            $errPtr = $ffi->mdix_get_last_error();
            $msg    = $errPtr !== null ? \FFI::string($errPtr) : 'formatSource failed';
            throw MdixError::fromMessage($msg);
        }

        $result = \FFI::string($ptr);
        $ffi->mdix_free_string($ptr);

        return $result;
    }

    /**
     * Remove all unnecessary whitespace and comments from raw .mdix source.
     * String literal contents are preserved.
     *
     * @throws MdixError if minification fails.
     */
    public static function minifySource(string $source): string
    {
        if ($source === '') {
            throw new MdixError('source must not be empty', ErrorKind::InvalidPath);
        }

        $ffi = NativeLoader::get();
        $ptr = $ffi->mdix_minify_source($source);

        if ($ptr === null) {
            $errPtr = $ffi->mdix_get_last_error();
            $msg    = $errPtr !== null ? \FFI::string($errPtr) : 'minifySource failed';
            throw MdixError::fromMessage($msg);
        }

        $result = \FFI::string($ptr);
        $ffi->mdix_free_string($ptr);

        return $result;
    }

    // ── Round-trip helpers ────────────────────────────────────────────────────

    /**
     * Export $db to JSON and immediately reload it.
     * Useful for stripping DixScript-specific metadata.
     * The caller must close the returned database.
     *
     * @throws MdixError on failure.
     */
    public static function jsonRoundTrip(MdixDatabase $db): MdixDatabase
    {
        return self::fromJson(self::toJson($db, false));
    }

    // ── Railway variants ──────────────────────────────────────────────────────

    public static function tryToJson(MdixDatabase $db, bool $indented = true): MdixResult
    {
        try {
            return MdixResult::ok(self::toJson($db, $indented));
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

    public static function tryFormatSource(
        string $source,
        FormatMode $mode = FormatMode::Default,
    ): MdixResult {
        try {
            return MdixResult::ok(self::formatSource($source, $mode));
        } catch (\Throwable $e) {
            return MdixResult::fromThrowable($e);
        }
    }

    // Prevent instantiation — all methods are static
    private function __construct() {}
}
