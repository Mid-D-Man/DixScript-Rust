package dixscript

import "testing"

type testEnemy struct {
	Name string `json:"name"`
	HP   int    `json:"hp"`
}

const enemiesFixture = `
@DATA(
  enemies::
    { name = "Goblin", hp = 50 }
    { name = "Orc", hp = 120 }
    { name = "Orc", hp = 120 }
    { name = "Dragon", hp = 900 }
    { name = "Skeleton", hp = 40 }
)`

func loadEnemyQuery(t *testing.T) (*Database, *Query[testEnemy]) {
	t.Helper()
	db, err := LoadStr(enemiesFixture)
	if err != nil {
		t.Fatalf("LoadStr: %v", err)
	}
	q, err := LoadQuery[testEnemy](db, "enemies")
	if err != nil {
		db.Close()
		t.Fatalf("LoadQuery: %v", err)
	}
	return db, q
}

func TestLoadQueryAndCount(t *testing.T) {
	db, q := loadEnemyQuery(t)
	defer db.Close()

	if q.Count() != 5 {
		t.Errorf("Count() = %d, want 5", q.Count())
	}
	if q.IsEmpty() {
		t.Error("IsEmpty() = true for a non-empty query")
	}
}

func TestQueryWhere(t *testing.T) {
	db, q := loadEnemyQuery(t)
	defer db.Close()

	heavies := q.Where(func(e testEnemy) bool { return e.HP > 100 })
	if heavies.Count() != 2 {
		t.Errorf("Where(hp > 100).Count() = %d, want 2", heavies.Count())
	}
}

func TestQuerySelect(t *testing.T) {
	db, q := loadEnemyQuery(t)
	defer db.Close()

	names := Select(q, func(e testEnemy) string { return e.Name })
	if len(names) != 5 || names[0] != "Goblin" || names[4] != "Skeleton" {
		t.Errorf("Select(Name) = %v", names)
	}
}

func TestQueryOrderByAndDesc(t *testing.T) {
	db, q := loadEnemyQuery(t)
	defer db.Close()

	asc := OrderBy(q, func(e testEnemy) int { return e.HP })
	got := Select(asc, func(e testEnemy) int { return e.HP })
	want := []int{40, 50, 120, 120, 900}
	if !intSliceEqual(got, want) {
		t.Errorf("OrderBy(hp) = %v, want %v", got, want)
	}

	desc := OrderByDesc(q, func(e testEnemy) int { return e.HP })
	gotDesc := Select(desc, func(e testEnemy) int { return e.HP })
	wantDesc := []int{900, 120, 120, 50, 40}
	if !intSliceEqual(gotDesc, wantDesc) {
		t.Errorf("OrderByDesc(hp) = %v, want %v", gotDesc, wantDesc)
	}
}

func TestQueryDistinct(t *testing.T) {
	db, q := loadEnemyQuery(t)
	defer db.Close()

	names := Select(q, func(e testEnemy) string { return e.Name })
	nameQuery := NewQuery(names)
	distinctNames := Distinct(nameQuery).ToSlice()
	want := []string{"Goblin", "Orc", "Dragon", "Skeleton"} // first-seen order, one Orc
	if !strSliceEqual(distinctNames, want) {
		t.Errorf("Distinct(names) = %v, want %v", distinctNames, want)
	}
}

func TestQueryGroupBy(t *testing.T) {
	db, q := loadEnemyQuery(t)
	defer db.Close()

	groups := GroupBy(q, func(e testEnemy) string { return e.Name })
	if len(groups) != 4 {
		t.Fatalf("len(groups) = %d, want 4", len(groups))
	}
	// "Orc" is the second distinct name to appear and has 2 members.
	if groups[1].Key != "Orc" || len(groups[1].Items) != 2 {
		t.Errorf("groups[1] = %+v, want Key=Orc with 2 items", groups[1])
	}
}

func TestQueryMinMaxByKey(t *testing.T) {
	db, q := loadEnemyQuery(t)
	defer db.Close()

	weakest, ok := MinByKey(q, func(e testEnemy) int { return e.HP })
	if !ok || weakest.Name != "Skeleton" {
		t.Errorf("MinByKey(hp) = %+v, %v; want Skeleton, true", weakest, ok)
	}

	strongest, ok := MaxByKey(q, func(e testEnemy) int { return e.HP })
	if !ok || strongest.Name != "Dragon" {
		t.Errorf("MaxByKey(hp) = %+v, %v; want Dragon, true", strongest, ok)
	}
}

func TestQuerySumAndAvg(t *testing.T) {
	db, q := loadEnemyQuery(t)
	defer db.Close()

	sum := SumInt(q, func(e testEnemy) int64 { return int64(e.HP) })
	const wantSum = 50 + 120 + 120 + 900 + 40
	if sum != wantSum {
		t.Errorf("SumInt(hp) = %d, want %d", sum, wantSum)
	}

	avg, ok := AvgFloat(q, func(e testEnemy) float64 { return float64(e.HP) })
	wantAvg := float64(wantSum) / 5
	if !ok || avg != wantAvg {
		t.Errorf("AvgFloat(hp) = %v, %v; want %v, true", avg, ok, wantAvg)
	}
}

func TestQuerySkipTakeFirstLastNth(t *testing.T) {
	db, q := loadEnemyQuery(t)
	defer db.Close()

	if first, ok := q.First(); !ok || first.Name != "Goblin" {
		t.Errorf("First() = %+v, %v; want Goblin, true", first, ok)
	}
	if last, ok := q.Last(); !ok || last.Name != "Skeleton" {
		t.Errorf("Last() = %+v, %v; want Skeleton, true", last, ok)
	}
	if nth, ok := q.Nth(2); !ok || nth.Name != "Orc" {
		t.Errorf("Nth(2) = %+v, %v; want Orc, true", nth, ok)
	}
	if skipped := q.Skip(3); skipped.Count() != 2 {
		t.Errorf("Skip(3).Count() = %d, want 2", skipped.Count())
	}
	if taken := q.Take(2); taken.Count() != 2 {
		t.Errorf("Take(2).Count() = %d, want 2", taken.Count())
	}
}

func TestQueryAnyAll(t *testing.T) {
	db, q := loadEnemyQuery(t)
	defer db.Close()

	if !q.Any(func(e testEnemy) bool { return e.HP > 800 }) {
		t.Error("Any(hp > 800) = false, want true (Dragon)")
	}
	if q.All(func(e testEnemy) bool { return e.HP > 800 }) {
		t.Error("All(hp > 800) = true, want false")
	}
}

func TestQueryOnEmptyResultSet(t *testing.T) {
	db, q := loadEnemyQuery(t)
	defer db.Close()

	empty := q.Where(func(e testEnemy) bool { return e.HP > 100000 })
	if !empty.IsEmpty() || empty.Count() != 0 {
		t.Errorf("empty query: IsEmpty()=%v Count()=%d, want true, 0", empty.IsEmpty(), empty.Count())
	}
	if _, ok := empty.First(); ok {
		t.Error("First() on empty query returned ok=true")
	}
	if empty.FirstOr(testEnemy{Name: "none"}).Name != "none" {
		t.Error("FirstOr on empty query didn't return the default")
	}
	if _, ok := MinByKey(empty, func(e testEnemy) int { return e.HP }); ok {
		t.Error("MinByKey on empty query returned ok=true")
	}
	if _, ok := AvgFloat(empty, func(e testEnemy) float64 { return float64(e.HP) }); ok {
		t.Error("AvgFloat on empty query returned ok=true")
	}
}

func TestQueryManyWildcard(t *testing.T) {
	db, err := LoadStr(`
@DATA(
  servers: web1 = "up", web2 = "down", web3 = "up"
)`)
	if err != nil {
		t.Fatalf("LoadStr: %v", err)
	}
	defer db.Close()

	results, err := QueryMany[string](db, "servers.*")
	if err != nil {
		t.Fatalf("QueryMany: %v", err)
	}
	if len(results) != 3 {
		t.Errorf("QueryMany(servers.*) returned %d results, want 3", len(results))
	}
}

func intSliceEqual(a, b []int) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

func strSliceEqual(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}
