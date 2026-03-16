<?php
declare(strict_types=1);

namespace MidManStudio\Mdix\Tests;

use MidManStudio\Mdix\ErrorKind;
use MidManStudio\Mdix\MdixDatabase;
use MidManStudio\Mdix\MdixError;
use MidManStudio\Mdix\ValueType;
use PHPUnit\Framework\TestCase;

/**
 * Integration tests for MdixDatabase.
 * Requires the native lib to be on MDIX_LIB_PATH or in mdix-php/lib/.
 */
final class DatabaseTest extends TestCase
{
    private const SIMPLE_SRC = <<<'MDIX'
@DATA(
  greeting = "hello"
  port     = 8080
  rate     = 1.5f
  pi       = 3.14159
  active   = true
  server: host = "localhost", ssl = false
  tags:: "alpha", "beta", "gamma"
)
MDIX;

    private MdixDatabase $db;

    protected function setUp(): void
    {
        $this->db = MdixDatabase::loadStr(self::SIMPLE_SRC);
    }

    protected function tearDown(): void
    {
        $this->db->close();
    }

    // ── Loading ───────────────────────────────────────────────────────────────

    public function testLoadStrValidIsValid(): void
    {
        self::assertTrue($this->db->isValid());
    }

    public function testLoadStrEmptyThrows(): void
    {
        $this->expectException(MdixError::class);
        MdixDatabase::loadStr('');
    }

    public function testLoadStrMalformedThrows(): void
    {
        $this->expectException(MdixError::class);
        MdixDatabase::loadStr('@@@INVALID###');
    }

    public function testEntryCountPositive(): void
    {
        self::assertGreaterThan(0, $this->db->entryCount());
    }

    public function testCloseIsIdempotent(): void
    {
        $db = MdixDatabase::loadStr('@DATA( x = 1 )');
        $db->close();
        $db->close(); // must not throw
        self::assertTrue(true);
    }

    // ── fromJson ──────────────────────────────────────────────────────────────

    public function testFromJsonValidObject(): void
    {
        $db = MdixDatabase::fromJson('{"port":9000,"host":"db.local","ssl":false}');
        try {
            self::assertSame(9000, $db->getInt('port'));
            self::assertSame('db.local', $db->getString('host'));
            self::assertFalse($db->getBool('ssl'));
        } finally {
            $db->close();
        }
    }

    public function testFromJsonEmptyThrows(): void
    {
        $this->expectException(MdixError::class);
        MdixDatabase::fromJson('');
    }

    public function testFromJsonArrayTopLevelThrows(): void
    {
        $this->expectException(MdixError::class);
        MdixDatabase::fromJson('[1,2,3]');
    }

    // ── fromToml ──────────────────────────────────────────────────────────────

    public function testFromTomlValid(): void
    {
        $db = MdixDatabase::fromToml("port = 8080\nhost = \"localhost\"\n");
        try {
            self::assertSame(8080, $db->getInt('port'));
            self::assertSame('localhost', $db->getString('host'));
        } finally {
            $db->close();
        }
    }

    public function testFromTomlEmptyThrows(): void
    {
        $this->expectException(MdixError::class);
        MdixDatabase::fromToml('');
    }

    // ── getString ─────────────────────────────────────────────────────────────

    public function testGetStringKnownPath(): void
    {
        self::assertSame('hello', $this->db->getString('greeting'));
    }

    public function testGetStringWithDefault(): void
    {
        self::assertSame('fallback', $this->db->getString('missing', 'fallback'));
    }

    public function testGetStringMissingThrows(): void
    {
        $this->expectException(MdixError::class);
        $this->db->getString('missing');
    }

    public function testGetStringEmptyPathThrows(): void
    {
        $this->expectException(MdixError::class);
        $this->db->getString('');
    }

    // ── getInt ────────────────────────────────────────────────────────────────

    public function testGetIntKnownPath(): void
    {
        self::assertSame(8080, $this->db->getInt('port'));
    }

    public function testGetIntWithDefaultPresent(): void
    {
        self::assertSame(8080, $this->db->getInt('port', -1));
    }

    public function testGetIntWithDefaultMissing(): void
    {
        self::assertSame(-1, $this->db->getInt('missing', -1));
    }

    // ── getFloat / getDouble ──────────────────────────────────────────────────

    public function testGetFloat(): void
    {
        self::assertEqualsWithDelta(1.5, $this->db->getFloat('rate'), 0.001);
    }

    public function testGetDouble(): void
    {
        self::assertEqualsWithDelta(3.14159, $this->db->getDouble('pi'), 0.00001);
    }

    // ── getBool ───────────────────────────────────────────────────────────────

    public function testGetBoolTrue(): void
    {
        self::assertTrue($this->db->getBool('active'));
    }

    public function testGetBoolFalse(): void
    {
        self::assertFalse($this->db->getBool('server.ssl'));
    }

    // ── nested paths ─────────────────────────────────────────────────────────

    public function testGetStringNestedPath(): void
    {
        self::assertSame('localhost', $this->db->getString('server.host'));
    }

    // ── array ─────────────────────────────────────────────────────────────────

    public function testArrayLength(): void
    {
        self::assertSame(3, $this->db->arrayLength('tags'));
    }

    public function testArrayLengthNotArrayThrows(): void
    {
        $this->expectException(MdixError::class);
        $this->db->arrayLength('port');
    }

    // ── ValueType ─────────────────────────────────────────────────────────────

    public function testValueTypeInt(): void
    {
        self::assertSame(ValueType::Int, $this->db->valueTypeAt('port'));
    }

    public function testValueTypeString(): void
    {
        self::assertSame(ValueType::String, $this->db->valueTypeAt('greeting'));
    }

    public function testValueTypeBool(): void
    {
        self::assertSame(ValueType::Bool, $this->db->valueTypeAt('active'));
    }

    public function testValueTypeArray(): void
    {
        self::assertSame(ValueType::Array, $this->db->valueTypeAt('tags'));
    }

    public function testValueTypeUnknown(): void
    {
        self::assertSame(ValueType::Unknown, $this->db->valueTypeAt('nope'));
    }

    // ── exists ────────────────────────────────────────────────────────────────

    public function testExistsPresent(): void
    {
        self::assertTrue($this->db->exists('port'));
    }

    public function testExistsAbsent(): void
    {
        self::assertFalse($this->db->exists('nope'));
    }

    // ── keys ──────────────────────────────────────────────────────────────────

    public function testKeysTopLevelNonEmpty(): void
    {
        self::assertNotEmpty($this->db->keys());
    }

    public function testKeysContainsKnownKey(): void
    {
        self::assertContains('port', $this->db->keys());
        self::assertContains('greeting', $this->db->keys());
    }

    // ── getJson ───────────────────────────────────────────────────────────────

    public function testGetJsonReturnsValidJson(): void
    {
        $raw    = $this->db->getJson('port');
        $parsed = \json_decode($raw, true);
        self::assertSame(8080, $parsed);
    }

    // ── closed state ─────────────────────────────────────────────────────────

    public function testGetStringAfterCloseThrows(): void
    {
        $db = MdixDatabase::loadStr('@DATA( x = "v" )');
        $db->close();

        $this->expectException(MdixError::class);
        $db->getString('x');
    }

    public function testGetStringAfterCloseErrorKind(): void
    {
        $db = MdixDatabase::loadStr('@DATA( x = "v" )');
        $db->close();

        try {
            $db->getString('x');
            self::fail('Expected MdixError');
        } catch (MdixError $e) {
            self::assertSame(ErrorKind::Closed, $e->kind);
        }
    }

    // ── export ────────────────────────────────────────────────────────────────

    public function testToJsonContainsValues(): void
    {
        $json   = $this->db->toJson(false);
        $parsed = \json_decode($json, true);
        self::assertSame(8080, $parsed['port']);
        self::assertSame('hello', $parsed['greeting']);
    }

    public function testToJsonIndentedHasNewlines(): void
    {
        self::assertStringContainsString("\n", $this->db->toJson(true));
    }

    public function testToTomlContainsValues(): void
    {
        $toml = $this->db->toToml();
        self::assertStringContainsString('8080', $toml);
        self::assertStringContainsString('hello', $toml);
    }

    public function testToJsonThenFromJsonRoundtrip(): void
    {
        $json = $this->db->toJson(false);
        $db2  = MdixDatabase::fromJson($json);
        try {
            self::assertSame($this->db->getInt('port'), $db2->getInt('port'));
            self::assertSame($this->db->getString('greeting'), $db2->getString('greeting'));
        } finally {
            $db2->close();
        }
    }

    public function testToTomlThenFromTomlRoundtrip(): void
    {
        $toml = $this->db->toToml();
        $db2  = MdixDatabase::fromToml($toml);
        try {
            self::assertSame($this->db->getInt('port'), $db2->getInt('port'));
        } finally {
            $db2->close();
        }
    }

    // ── railway variants ──────────────────────────────────────────────────────

    public function testTryLoadStrSuccess(): void
    {
        $result = MdixDatabase::tryLoadStr('@DATA( port = 8080 )');
        self::assertTrue($result->isSuccess());
        $result->getValue()->close();
    }

    public function testTryLoadStrFailure(): void
    {
        $result = MdixDatabase::tryLoadStr('');
        self::assertTrue($result->isFailure());
    }

    public function testRailwayChainLoadAndGet(): void
    {
        $port = MdixDatabase::tryLoadStr('@DATA( port = 8080 )')
            ->andThen(fn ($db) => $db->tryGetInt('port'))
            ->unwrapOr(0);

        self::assertSame(8080, $port);
    }

    public function testRailwayChainWithMap(): void
    {
        $portX2 = MdixDatabase::tryLoadStr('@DATA( port = 4040 )')
            ->andThen(fn ($db) => $db->tryGetInt('port'))
            ->map(fn ($p) => $p * 2)
            ->unwrapOr(0);

        self::assertSame(8080, $portX2);
    }

    public function testRailwayEnsureFailsWhenPredicateFalse(): void
    {
        $result = MdixDatabase::tryLoadStr('@DATA( port = 80 )')
            ->andThen(fn ($db) => $db->tryGetInt('port'))
            ->ensure(fn ($p) => $p > 1024, 'port must be > 1024');

        self::assertTrue($result->isFailure());
        self::assertStringContainsString('1024', $result->getError());
    }
}
