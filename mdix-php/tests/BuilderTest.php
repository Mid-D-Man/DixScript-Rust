<?php
declare(strict_types=1);

namespace MidManStudio\Mdix\Tests;

use MidManStudio\Mdix\ErrorKind;
use MidManStudio\Mdix\MdixBuilder;
use MidManStudio\Mdix\MdixError;
use PHPUnit\Framework\TestCase;

final class BuilderTest extends TestCase
{
    // ── Lifecycle ─────────────────────────────────────────────────────────────

    public function testNewBuilderIsNotNull(): void
    {
        $b = new MdixBuilder();
        self::assertNotNull($b);
        $b->close();
    }

    public function testCloseCalledTwiceDoesNotThrow(): void
    {
        $b = new MdixBuilder();
        $b->close();
        $b->close();
        self::assertTrue(true);
    }

    public function testSetStringAfterCloseThrows(): void
    {
        $b = new MdixBuilder();
        $b->close();

        $this->expectException(MdixError::class);
        $b->setString('x', 'v');
    }

    public function testSetStringAfterCloseErrorKind(): void
    {
        $b = new MdixBuilder();
        $b->close();

        try {
            $b->setString('x', 'v');
            self::fail('Expected MdixError');
        } catch (MdixError $e) {
            self::assertSame(ErrorKind::Closed, $e->kind);
        }
    }

    // ── Set / has key ─────────────────────────────────────────────────────────

    public function testSetStringHasKey(): void
    {
        $b = new MdixBuilder();
        $b->setString('app.name', 'DixScript');
        self::assertTrue($b->hasKey('app.name'));
        $b->close();
    }

    public function testSetIntHasKey(): void
    {
        $b = new MdixBuilder();
        $b->setInt('port', 8080);
        self::assertTrue($b->hasKey('port'));
        $b->close();
    }

    public function testSetFloatHasKey(): void
    {
        $b = new MdixBuilder();
        $b->setFloat('rate', 1.5);
        self::assertTrue($b->hasKey('rate'));
        $b->close();
    }

    public function testSetDoubleHasKey(): void
    {
        $b = new MdixBuilder();
        $b->setDouble('pi', 3.14159);
        self::assertTrue($b->hasKey('pi'));
        $b->close();
    }

    public function testSetBoolHasKey(): void
    {
        $b = new MdixBuilder();
        $b->setBool('debug', true);
        self::assertTrue($b->hasKey('debug'));
        $b->close();
    }

    // ── Fluent chaining ───────────────────────────────────────────────────────

    public function testFluentChainWorks(): void
    {
        $b = new MdixBuilder();
        $b->setString('a', 'hello')
          ->setInt('b', 42)
          ->setBool('c', false);
        self::assertTrue($b->hasKey('a'));
        self::assertTrue($b->hasKey('b'));
        self::assertTrue($b->hasKey('c'));
        $b->close();
    }

    // ── Remove ────────────────────────────────────────────────────────────────

    public function testRemoveExistingKeyReturnsTrue(): void
    {
        $b = new MdixBuilder();
        $b->setInt('x', 1);
        self::assertTrue($b->remove('x'));
        self::assertFalse($b->hasKey('x'));
        $b->close();
    }

    public function testRemoveMissingKeyReturnsFalse(): void
    {
        $b = new MdixBuilder();
        self::assertFalse($b->remove('nope'));
        $b->close();
    }

    // ── Clear ─────────────────────────────────────────────────────────────────

    public function testClearRemovesAllKeys(): void
    {
        $b = new MdixBuilder();
        $b->setString('a', '1')->setInt('b', 2)->setBool('c', true);
        $b->clear();
        self::assertFalse($b->hasKey('a'));
        self::assertFalse($b->hasKey('b'));
        self::assertFalse($b->hasKey('c'));
        $b->close();
    }

    // ── toMdixString / toDatabase ─────────────────────────────────────────────

    public function testToMdixStringNonEmpty(): void
    {
        $b = new MdixBuilder();
        $b->setString('name', 'test')->setInt('val', 99);
        self::assertNotEmpty($b->toMdixString());
        $b->close();
    }

    public function testToDatabaseValuesReadable(): void
    {
        $b = new MdixBuilder();
        $b->setString('greet', 'hello')
          ->setInt('num', 7)
          ->setBool('flag', true);

        $db = $b->toDatabase();
        try {
            self::assertSame('hello', $db->getString('greet'));
            self::assertSame(7, $db->getInt('num'));
            self::assertTrue($db->getBool('flag'));
        } finally {
            $db->close();
            $b->close();
        }
    }

    public function testToDatabaseMultipleTypes(): void
    {
        $b = new MdixBuilder();
        $b->setString('s', 'world')
          ->setInt('i', 42)
          ->setFloat('f', 1.5)
          ->setDouble('d', 3.14)
          ->setBool('b', false);

        $db = $b->toDatabase();
        try {
            self::assertSame('world', $db->getString('s'));
            self::assertSame(42, $db->getInt('i'));
            self::assertEqualsWithDelta(1.5, $db->getFloat('f'), 0.001);
            self::assertEqualsWithDelta(3.14, $db->getDouble('d'), 0.001);
            self::assertFalse($db->getBool('b'));
        } finally {
            $db->close();
            $b->close();
        }
    }

    // ── saveToFile ────────────────────────────────────────────────────────────

    public function testSaveToFileWritesFile(): void
    {
        $b    = new MdixBuilder();
        $path = \sys_get_temp_dir() . '/mdix_test_' . \uniqid() . '.mdix';

        try {
            $b->setString('saved', 'yes');
            $b->saveToFile($path);
            self::assertFileExists($path);
        } finally {
            if (\file_exists($path)) {
                \unlink($path);
            }
            $b->close();
        }
    }

    public function testSaveToFileLoadBack(): void
    {
        $b    = new MdixBuilder();
        $path = \sys_get_temp_dir() . '/mdix_test_' . \uniqid() . '.mdix';

        try {
            $b->setInt('answer', 42);
            $b->saveToFile($path);
            $b->close();

            $db = \MidManStudio\Mdix\MdixDatabase::load($path);
            try {
                self::assertSame(42, $db->getInt('answer'));
            } finally {
                $db->close();
            }
        } finally {
            if (\file_exists($path)) {
                \unlink($path);
            }
        }
    }

    // ── null / empty path guards ──────────────────────────────────────────────

    public function testSetStringEmptyPathThrows(): void
    {
        $b = new MdixBuilder();
        try {
            $this->expectException(MdixError::class);
            $b->setString('', 'v');
        } finally {
            $b->close();
        }
    }

    public function testSetIntEmptyPathThrows(): void
    {
        $b = new MdixBuilder();
        try {
            $this->expectException(MdixError::class);
            $b->setInt('', 1);
        } finally {
            $b->close();
        }
    }

    // ── tryToDatabase ─────────────────────────────────────────────────────────

    public function testTryToDatabaseSuccess(): void
    {
        $b      = new MdixBuilder();
        $result = $b->setInt('port', 8080)->tryToDatabase();
        self::assertTrue($result->isSuccess());
        $result->getValue()->close();
        $b->close();
    }

    // ── date / timestamp helpers ──────────────────────────────────────────────

    public function testSetDateStoresString(): void
    {
        $b    = new MdixBuilder();
        $date = new \DateTimeImmutable('2025-06-15');
        $b->setDate('release', $date);
        self::assertTrue($b->hasKey('release'));
        $b->close();
    }

    public function testSetTimestampStoresString(): void
    {
        $b  = new MdixBuilder();
        $ts = new \DateTimeImmutable('2025-06-15T10:30:00Z');
        $b->setTimestamp('created_at', $ts);
        self::assertTrue($b->hasKey('created_at'));
        $b->close();
    }
}
