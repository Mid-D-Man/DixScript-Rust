<?php
declare(strict_types=1);

namespace MidManStudio\Mdix\Tests;

use MidManStudio\Mdix\ArrayMergeStrategy;
use MidManStudio\Mdix\MdixDatabase;
use MidManStudio\Mdix\MdixError;
use MidManStudio\Mdix\MdixMerge;
use MidManStudio\Mdix\MergeStrategy;
use PHPUnit\Framework\TestCase;

/**
 * Integration tests for MdixMerge.
 * Requires the native lib to be on MDIX_LIB_PATH or in mdix-php/lib/.
 */
final class MergeTest extends TestCase
{
    private const BASE = '@DATA( app_name = "MyApp" server: host = "localhost", port = 8080 tags:: "a", "b" )';
    private const OVERRIDE = '@DATA( server: port = 9090, ssl = true tags:: "c" )';

    public function testSourcesDefaultWeightsPrimaryWinsSharedKey(): void
    {
        $result = MdixMerge::sources(self::BASE, self::OVERRIDE);
        try {
            // Auto-descending weights: source[0] outweighs source[1] on the shared "port" key.
            self::assertSame(8080, $result->database->getInt('server.port'));
            self::assertSame('localhost', $result->database->getString('server.host'));
            self::assertTrue($result->database->getBool('server.ssl')); // not a conflict -- only OVERRIDE defines it
        } finally {
            $result->database->close();
        }
    }

    public function testSourcesReportsConflictOnSharedKey(): void
    {
        $result = MdixMerge::sources(self::BASE, self::OVERRIDE);
        try {
            self::assertTrue($result->hasConflicts());
            $found = false;
            foreach ($result->conflicts as $c) {
                if (\str_contains($c->path, 'port')) {
                    $found = true;
                }
            }
            self::assertTrue($found);
        } finally {
            $result->database->close();
        }
    }

    public function testSourcesWeightedSecondaryWinsOverrideTakesPriority(): void
    {
        $result = MdixMerge::sourcesWeighted(
            [self::BASE, self::OVERRIDE], null, MergeStrategy::SecondaryWins, ArrayMergeStrategy::Replace,
        );
        try {
            self::assertSame(9090, $result->database->getInt('server.port'));
        } finally {
            $result->database->close();
        }
    }

    public function testSourcesWeightedPrimaryWinsBaseTakesPriority(): void
    {
        $result = MdixMerge::sourcesWeighted(
            [self::BASE, self::OVERRIDE], null, MergeStrategy::PrimaryWins, ArrayMergeStrategy::Replace,
        );
        try {
            self::assertSame(8080, $result->database->getInt('server.port'));
        } finally {
            $result->database->close();
        }
    }

    public function testSourcesWeightedExplicitWeightsHigherWeightWins(): void
    {
        $result = MdixMerge::sourcesWeighted(
            [self::BASE, self::OVERRIDE], [0.2, 0.9], MergeStrategy::WeightedPriority, ArrayMergeStrategy::Replace,
        );
        try {
            self::assertSame(9090, $result->database->getInt('server.port'));
        } finally {
            $result->database->close();
        }
    }

    public function testSourcesWeightedThrowOnConflictThrowsWhenSharedKeyExists(): void
    {
        $this->expectException(MdixError::class);
        MdixMerge::sourcesWeighted(
            [self::BASE, self::OVERRIDE], null, MergeStrategy::ThrowOnConflict, ArrayMergeStrategy::Replace,
        );
    }

    public function testSourcesWeightedMismatchedWeightsLengthThrows(): void
    {
        $this->expectException(MdixError::class);
        MdixMerge::sourcesWeighted([self::BASE, self::OVERRIDE], [1.0]);
    }

    public function testArrayStrategyReplaceWinnerReplacesArray(): void
    {
        $result = MdixMerge::sourcesWeighted(
            [self::BASE, self::OVERRIDE], null, MergeStrategy::WeightedPriority, ArrayMergeStrategy::Replace,
        );
        try {
            self::assertSame(2, $result->database->arrayLength('tags')); // BASE's ["a","b"] wins outright
        } finally {
            $result->database->close();
        }
    }

    public function testArrayStrategyConcatCombinesBothArrays(): void
    {
        $result = MdixMerge::sourcesWeighted(
            [self::BASE, self::OVERRIDE], null, MergeStrategy::WeightedPriority, ArrayMergeStrategy::Concat,
        );
        try {
            self::assertSame(3, $result->database->arrayLength('tags')); // ["a","b"] ++ ["c"]
        } finally {
            $result->database->close();
        }
    }

    public function testDatabasesRoundTripsThroughSourceText(): void
    {
        $base = MdixDatabase::loadStr(self::BASE);
        $override = MdixDatabase::loadStr(self::OVERRIDE);
        try {
            $result = MdixMerge::databases($base, $override);
            try {
                self::assertSame('MyApp', $result->database->getString('app_name'));
            } finally {
                $result->database->close();
            }
        } finally {
            $base->close();
            $override->close();
        }
    }

    public function testSourcesEmptyArrayThrows(): void
    {
        $this->expectException(MdixError::class);
        MdixMerge::sources();
    }

    public function testSourcesMalformedSourceThrows(): void
    {
        $this->expectException(MdixError::class);
        MdixMerge::sources(self::BASE, '@@@INVALID$$$');
    }
}
