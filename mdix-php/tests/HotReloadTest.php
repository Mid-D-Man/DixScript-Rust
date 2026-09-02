<?php
declare(strict_types=1);

namespace MidManStudio\Mdix\Tests;

use MidManStudio\Mdix\MdixError;
use MidManStudio\Mdix\MdixHotReload;
use PHPUnit\Framework\TestCase;

/**
 * Integration tests for MdixHotReload.
 * Requires the native lib to be on MDIX_LIB_PATH or in mdix-php/lib/.
 *
 * Modified-time changes are set explicitly via touch() rather than relying
 * on real-time sleeps between writes -- several filesystems only offer
 * one-second mtime granularity, which would make a sleep-based test both
 * slow and flaky.
 */
final class HotReloadTest extends TestCase
{
    private string $tmpFile;

    protected function setUp(): void
    {
        $this->tmpFile = \sys_get_temp_dir() . '/mdix_hotreload_test_' . \bin2hex(\random_bytes(8)) . '.mdix';
    }

    protected function tearDown(): void
    {
        if (\file_exists($this->tmpFile)) {
            \unlink($this->tmpFile);
        }
    }

    private function write(string $content, int $mtime): void
    {
        \file_put_contents($this->tmpFile, $content);
        \touch($this->tmpFile, $mtime);
    }

    public function testHasChangedTrueBeforeFirstReload(): void
    {
        $this->write('@DATA( port = 8080 )', \time());

        $watcher = new MdixHotReload($this->tmpFile);
        try {
            self::assertFalse($watcher->hasLoaded());
            self::assertTrue($watcher->hasChanged());
        } finally {
            $watcher->close();
        }
    }

    public function testCheckAndReloadFirstCallReloadsAndReturnsData(): void
    {
        $this->write('@DATA( port = 8080 )', \time());

        $watcher = new MdixHotReload($this->tmpFile);
        try {
            $db = $watcher->checkAndReload();
            self::assertNotNull($db);
            try {
                self::assertSame(8080, $db->getInt('port'));
            } finally {
                $db->close();
            }
            self::assertTrue($watcher->hasLoaded());
        } finally {
            $watcher->close();
        }
    }

    public function testCheckAndReloadNoChangeReturnsNull(): void
    {
        $t0 = \time();
        $this->write('@DATA( port = 8080 )', $t0);

        $watcher = new MdixHotReload($this->tmpFile);
        try {
            $watcher->checkAndReload()?->close();
            self::assertFalse($watcher->hasChanged());
            self::assertNull($watcher->checkAndReload());
        } finally {
            $watcher->close();
        }
    }

    public function testCheckAndReloadAfterModificationReloadsAgain(): void
    {
        $t0 = \time();
        $this->write('@DATA( port = 8080 )', $t0);

        $watcher = new MdixHotReload($this->tmpFile);
        try {
            $watcher->checkAndReload()?->close();

            $this->write('@DATA( port = 9090 )', $t0 + 5);

            $db = $watcher->checkAndReload();
            self::assertNotNull($db);
            try {
                self::assertSame(9090, $db->getInt('port'));
            } finally {
                $db->close();
            }
        } finally {
            $watcher->close();
        }
    }

    public function testForceReloadReloadsRegardlessOfChange(): void
    {
        $this->write('@DATA( port = 8080 )', \time());

        $watcher = new MdixHotReload($this->tmpFile);
        try {
            $watcher->checkAndReload()?->close();
            $db = $watcher->forceReload();
            try {
                self::assertSame(8080, $db->getInt('port'));
            } finally {
                $db->close();
            }
        } finally {
            $watcher->close();
        }
    }

    public function testPathReturnsConstructorPath(): void
    {
        $this->write('@DATA( port = 8080 )', \time());

        $watcher = new MdixHotReload($this->tmpFile);
        try {
            self::assertSame($this->tmpFile, $watcher->path());
        } finally {
            $watcher->close();
        }
    }

    public function testMalformedFileReloadFailsAndRetriesOnNextCall(): void
    {
        $t0 = \time();
        $this->write('@@@INVALID$$$', $t0);

        $watcher = new MdixHotReload($this->tmpFile);
        try {
            $threw = false;
            try {
                $watcher->checkAndReload();
            } catch (MdixError) {
                $threw = true;
            }
            self::assertTrue($threw);

            // Failure must not have consumed the "changed" state -- a fix-and-retry should still see a change.
            $this->write('@DATA( port = 1 )', $t0 + 5);
            self::assertNotNull($watcher->checkAndReload());
        } finally {
            $watcher->close();
        }
    }

    public function testMissingFileHasChangedThrows(): void
    {
        $watcher = new MdixHotReload(\sys_get_temp_dir() . '/does_not_exist_xyz.mdix');
        try {
            $this->expectException(MdixError::class);
            $watcher->hasChanged();
        } finally {
            $watcher->close();
        }
    }

    public function testClosedWatcherThrowsOnFurtherUse(): void
    {
        $this->write('@DATA( port = 8080 )', \time());

        $watcher = new MdixHotReload($this->tmpFile);
        $watcher->close();

        $this->expectException(MdixError::class);
        $watcher->hasChanged();
    }

    public function testCloseIsIdempotent(): void
    {
        $this->write('@DATA( port = 8080 )', \time());

        $watcher = new MdixHotReload($this->tmpFile);
        $watcher->close();
        $watcher->close();
        $this->addToAssertionCount(1);
    }
}
