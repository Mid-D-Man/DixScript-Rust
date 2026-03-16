<?php
declare(strict_types=1);

namespace MidManStudio\Mdix\Internal;

/**
 * Loads the mdix_ffi native library via PHP FFI and returns a singleton FFI
 * instance. The library is located by checking (in order):
 *
 *   1. The MDIX_LIB_PATH environment variable (absolute path to the .so/.dll).
 *   2. mdix-php/lib/<platform-lib-name> — populated by CI or local cargo build.
 *   3. The OS library loader (LD_LIBRARY_PATH, PATH, etc.) as a last resort.
 *
 * Requirements:
 *   - PHP 8.1+
 *   - ext-ffi enabled in php.ini (extension=ffi)
 *   - ffi.enable = true  OR  ffi.enable = preload  in php.ini
 */
final class NativeLoader
{
    private static ?\FFI $instance = null;

    /**
     * Returns the shared FFI instance, loading the native library on first call.
     *
     * @throws \RuntimeException if the FFI extension is unavailable or the lib
     *                           cannot be found.
     */
    public static function get(): \FFI
    {
        if (self::$instance === null) {
            self::$instance = self::load();
        }

        return self::$instance;
    }

    /**
     * Reset the singleton — intended for testing only.
     * @internal
     */
    public static function reset(): void
    {
        self::$instance = null;
    }

    // ── Private ───────────────────────────────────────────────────────────────

    private static function load(): \FFI
    {
        if (!\extension_loaded('ffi')) {
            throw new \RuntimeException(
                'The PHP FFI extension is required by MidManStudio\Mdix. '
                . 'Enable it by adding "extension=ffi" and "ffi.enable=true" '
                . 'to your php.ini.'
            );
        }

        $headerPath = __DIR__ . '/ffi_header.h';
        if (!\is_file($headerPath)) {
            throw new \RuntimeException(
                "FFI header not found at: {$headerPath}"
            );
        }

        $header  = \file_get_contents($headerPath);
        $libPath = self::resolveLibPath();

        try {
            return \FFI::cdef($header, $libPath);
        } catch (\FFI\Exception $e) {
            throw new \RuntimeException(
                "Failed to load mdix_ffi native library from '{$libPath}': "
                . $e->getMessage(),
                0,
                $e
            );
        }
    }

    private static function resolveLibPath(): string
    {
        // 1. Explicit override via environment variable
        $envPath = \getenv('MDIX_LIB_PATH');
        if ($envPath !== false && $envPath !== '' && \is_file($envPath)) {
            return \realpath($envPath) ?: $envPath;
        }

        // 2. Bundled lib: mdix-php/lib/<libname>
        //    __DIR__ = src/Mdix/Internal — go up three levels to mdix-php/
        $libName = self::platformLibName();
        $bundled = \realpath(__DIR__ . '/../../../lib/' . $libName);
        if ($bundled !== false && \is_file($bundled)) {
            return $bundled;
        }

        // 3. Fallback — let the OS loader resolve it
        return $libName;
    }

    private static function platformLibName(): string
    {
        return match (\PHP_OS_FAMILY) {
            'Windows' => 'mdix_ffi.dll',
            'Darwin'  => 'libmdix_ffi.dylib',
            default   => 'libmdix_ffi.so',
        };
    }

    private function __construct() {}
}
