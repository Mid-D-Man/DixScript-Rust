<?php
declare(strict_types=1);

namespace MidManStudio\Mdix\Tests;

use MidManStudio\Mdix\FormatMode;
use MidManStudio\Mdix\MdixConverter;
use MidManStudio\Mdix\MdixDatabase;
use MidManStudio\Mdix\MdixError;
use PHPUnit\Framework\TestCase;

final class ConverterTest extends TestCase
{
    private const SRC = '@DATA( port = 8080, host = "localhost", ssl = true )';

    // ── toJson ────────────────────────────────────────────────────────────────

    public function testToJsonIndentedContainsNewlines(): void
    {
        $db = MdixDatabase::loadStr(self::SRC);
        try {
            $json = MdixConverter::toJson($db, true);
            self::assertStringContainsString("\n", $json);
            self::assertStringContainsString('8080', $json);
            self::assertStringContainsString('localhost', $json);
        } finally {
            $db->close();
        }
    }

    public function testToJsonCompactNoNewlines(): void
    {
        $db = MdixDatabase::loadStr(self::SRC);
        try {
            $json = MdixConverter::toJson($db, false);
            self::assertStringNotContainsString("\n", \trim($json));
            self::assertStringContainsString('8080', $json);
        } finally {
            $db->close();
        }
    }

    // ── fromJson ──────────────────────────────────────────────────────────────

    public function testFromJsonValidObjectReadable(): void
    {
        $db = MdixConverter::fromJson('{"port":9000,"host":"db.local","ssl":false}');
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
        MdixConverter::fromJson('');
    }

    public function testFromJsonInvalidThrows(): void
    {
        $this->expectException(MdixError::class);
        MdixConverter::fromJson('not json at all');
    }

    // ── toToml ────────────────────────────────────────────────────────────────

    public function testToTomlContainsValues(): void
    {
        $db = MdixDatabase::loadStr(self::SRC);
        try {
            $toml = MdixConverter::toToml($db);
            self::assertStringContainsString('8080', $toml);
            self::assertStringContainsString('localhost', $toml);
        } finally {
            $db->close();
        }
    }

    // ── fromToml ──────────────────────────────────────────────────────────────

    public function testFromTomlValidTableReadable(): void
    {
        $db = MdixConverter::fromToml("port = 7070\nhost = \"toml.local\"\nssl = true\n");
        try {
            self::assertSame(7070, $db->getInt('port'));
            self::assertSame('toml.local', $db->getString('host'));
            self::assertTrue($db->getBool('ssl'));
        } finally {
            $db->close();
        }
    }

    public function testFromTomlEmptyThrows(): void
    {
        $this->expectException(MdixError::class);
        MdixConverter::fromToml('');
    }

    // ── round-trip ────────────────────────────────────────────────────────────

    public function testJsonRoundTripValuesPreserved(): void
    {
        $original = MdixDatabase::loadStr(self::SRC);
        $restored = MdixConverter::jsonRoundTrip($original);
        try {
            self::assertSame($original->getInt('port'), $restored->getInt('port'));
            self::assertSame($original->getString('host'), $restored->getString('host'));
            self::assertSame($original->getBool('ssl'), $restored->getBool('ssl'));
        } finally {
            $original->close();
            $restored->close();
        }
    }

    public function testToJsonThenFromJsonRoundTrips(): void
    {
        $original = MdixDatabase::loadStr(self::SRC);
        try {
            $json     = MdixConverter::toJson($original, false);
            $restored = MdixConverter::fromJson($json);
            try {
                self::assertSame(8080, $restored->getInt('port'));
                self::assertSame('localhost', $restored->getString('host'));
            } finally {
                $restored->close();
            }
        } finally {
            $original->close();
        }
    }

    public function testToTomlThenFromTomlRoundTrips(): void
    {
        $original = MdixDatabase::loadStr(self::SRC);
        try {
            $toml     = MdixConverter::toToml($original);
            $restored = MdixConverter::fromToml($toml);
            try {
                self::assertSame(8080, $restored->getInt('port'));
                self::assertSame('localhost', $restored->getString('host'));
            } finally {
                $restored->close();
            }
        } finally {
            $original->close();
        }
    }

    // ── toMdix ────────────────────────────────────────────────────────────────

    public function testToMdixDefaultContainsDataSection(): void
    {
        $db = MdixDatabase::loadStr(self::SRC);
        try {
            $mdix = MdixConverter::toMdix($db, FormatMode::Default);
            self::assertStringContainsString('@DATA(', $mdix);
            self::assertStringContainsString('8080', $mdix);
        } finally {
            $db->close();
        }
    }

    public function testToMdixMinifiedShorterThanDefault(): void
    {
        $db = MdixDatabase::loadStr(self::SRC);
        try {
            $normal   = MdixConverter::toMdix($db, FormatMode::Default);
            $minified = MdixConverter::toMdix($db, FormatMode::Minified);
            self::assertLessThan(\strlen($normal), \strlen($minified));
        } finally {
            $db->close();
        }
    }

    // ── formatSource ─────────────────────────────────────────────────────────

    public function testMinifySourceRemovesComments(): void
    {
        $src    = "@DATA( x = 1 // comment\n)";
        $result = MdixConverter::minifySource($src);
        self::assertStringNotContainsString('//', $result);
        self::assertStringContainsString('x', $result);
    }

    public function testFormatSourceEmptyThrows(): void
    {
        $this->expectException(MdixError::class);
        MdixConverter::formatSource('');
    }

    public function testMinifySourceEmptyThrows(): void
    {
        $this->expectException(MdixError::class);
        MdixConverter::minifySource('');
    }

    // ── railway variants ──────────────────────────────────────────────────────

    public function testTryToJsonSuccess(): void
    {
        $db     = MdixDatabase::loadStr(self::SRC);
        $result = MdixConverter::tryToJson($db, false);
        try {
            self::assertTrue($result->isSuccess());
            self::assertStringContainsString('8080', $result->getValue());
        } finally {
            $db->close();
        }
    }

    public function testTryFromJsonSuccess(): void
    {
        $result = MdixConverter::tryFromJson('{"score":99}');
        self::assertTrue($result->isSuccess());
        $result->getValue()->close();
    }

    public function testTryFromJsonFailure(): void
    {
        $result = MdixConverter::tryFromJson('');
        self::assertTrue($result->isFailure());
    }

    public function testTryFromTomlSuccess(): void
    {
        $result = MdixConverter::tryFromToml("retries = 3\n");
        self::assertTrue($result->isSuccess());
        self::assertSame(3, $result->getValue()->getInt('retries'));
        $result->getValue()->close();
    }
}
