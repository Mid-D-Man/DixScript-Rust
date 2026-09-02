<?php
declare(strict_types=1);

namespace MidManStudio\Mdix\Tests;

use MidManStudio\Mdix\MdixDatabase;
use PHPUnit\Framework\TestCase;

/**
 * Integration tests for MdixDatabase::query()/queryMany() and MdixQuery.
 * Requires the native lib to be on MDIX_LIB_PATH or in mdix-php/lib/.
 */
final class QueryTest extends TestCase
{
    private const ENEMIES_SRC = <<<'MDIX'
@DATA(
  world = "overworld"
  tags:: "alpha", "beta", "alpha", "gamma"
  enemies::
    { name = "Goblin", health = 50,   aiType = "AGGRESSIVE" },
    { name = "Orc",     health = 100, aiType = "AGGRESSIVE" },
    { name = "Dragon",  health = 1000, aiType = "BOSS" },
    { name = "Slime",   health = 20,  aiType = "PASSIVE" }
  levels::
    { id = 1, enemies:: { name = "Rat", health = 5, aiType = "PASSIVE" } },
    { id = 2, enemies:: { name = "Bat", health = 8, aiType = "PASSIVE" } }
)
MDIX;

    private MdixDatabase $db;

    protected function setUp(): void
    {
        $this->db = MdixDatabase::loadStr(self::ENEMIES_SRC);
    }

    protected function tearDown(): void
    {
        $this->db->close();
    }

    public function testQueryReturnsAllElements(): void
    {
        self::assertSame(4, $this->db->query('enemies')->count());
    }

    public function testWhereFiltersByField(): void
    {
        $aggressive = $this->db->query('enemies')
            ->where(fn (array $e): bool => $e['aiType'] === 'AGGRESSIVE')
            ->select(fn (array $e): string => $e['name']);

        self::assertSame(['Goblin', 'Orc'], $aggressive);
    }

    public function testWhereFieldEqualsShorthand(): void
    {
        self::assertSame(1, $this->db->query('enemies')->whereFieldEquals('aiType', 'BOSS')->count());
    }

    public function testSelectProjectsField(): void
    {
        self::assertSame(
            ['Goblin', 'Orc', 'Dragon', 'Slime'],
            $this->db->query('enemies')->selectField('name'),
        );
    }

    public function testOrderBySortsAscending(): void
    {
        $healths = $this->db->query('enemies')
            ->orderBy(fn (array $e): int => $e['health'])
            ->selectField('health');

        self::assertSame([20, 50, 100, 1000], $healths);
    }

    public function testOrderByDescendingSortsDescending(): void
    {
        $first = $this->db->query('enemies')->orderByDescending(fn (array $e): int => $e['health'])->first();
        self::assertSame('Dragon', $first['name']);
    }

    public function testTakeLimitsResults(): void
    {
        self::assertSame(2, $this->db->query('enemies')->take(2)->count());
    }

    public function testSkipDropsLeadingResults(): void
    {
        self::assertSame(1, $this->db->query('enemies')->skip(3)->count());
    }

    public function testAnyTrue(): void
    {
        self::assertTrue($this->db->query('enemies')->any(fn (array $e): bool => $e['aiType'] === 'BOSS'));
    }

    public function testAllFalse(): void
    {
        self::assertFalse($this->db->query('enemies')->all(fn (array $e): bool => $e['aiType'] === 'AGGRESSIVE'));
    }

    public function testCountWithFilter(): void
    {
        $count = $this->db->query('enemies')->where(fn (array $e): bool => $e['aiType'] === 'AGGRESSIVE')->count();
        self::assertSame(2, $count);
    }

    public function testIsEmptyFalseWhenPopulated(): void
    {
        self::assertFalse($this->db->query('enemies')->isEmpty());
    }

    public function testFirstAndLast(): void
    {
        self::assertSame('Goblin', $this->db->query('enemies')->first()['name']);
        self::assertSame('Slime', $this->db->query('enemies')->last()['name']);
    }

    public function testNthReturnsElementAtIndex(): void
    {
        self::assertSame('Dragon', $this->db->query('enemies')->nth(2)['name']);
    }

    public function testNthOutOfRangeReturnsNull(): void
    {
        self::assertNull($this->db->query('enemies')->nth(99));
    }

    public function testSumIntSumsHealth(): void
    {
        $sum = \array_sum($this->db->query('enemies')->selectField('health'));
        self::assertSame(1170, $sum);
    }

    public function testGroupByGroupsByField(): void
    {
        $byAi = $this->db->query('enemies')->groupBy(fn (array $e): string => $e['aiType']);
        self::assertCount(2, $byAi['AGGRESSIVE']);
        self::assertCount(1, $byAi['BOSS']);
        self::assertCount(1, $byAi['PASSIVE']);
    }

    public function testDistinctRemovesDuplicateScalars(): void
    {
        self::assertSame(4, $this->db->query('tags')->count());
        self::assertSame(3, $this->db->query('tags')->distinct()->count());
    }

    public function testQueryManyNonMatchingPatternReturnsEmpty(): void
    {
        self::assertTrue($this->db->queryMany('no.such.path.*')->isEmpty());
    }

    public function testQueryManyGlobPatternDoesNotThrow(): void
    {
        // Exact glob-matcher semantics live in dixscript::Runtime::DixData::select_many;
        // this is a smoke test that the native call round-trips cleanly end to end.
        $this->db->queryMany('levels.*.enemies');
        $this->addToAssertionCount(1);
    }

    public function testToArrayDropsToPlainPhpArray(): void
    {
        $high = \array_filter(
            $this->db->query('enemies')->toArray(),
            fn (array $e): bool => $e['health'] > 30,
        );
        self::assertCount(3, $high);
    }
}
