package dixscript

import "testing"

// TestValueTypeMatchesFFIDiscriminants is the regression test for the bug
// found while building query.go: ValueType previously skipped Long
// entirely (Int=2, Float=3, ...), shifting every type from Float onward
// one discriminant off from MdixType in mdix-ffi/src/lib.rs. Loads one
// field of every type and asserts ValueTypeAt reports exactly what the
// FFI actually assigned it — not just "some non-error value".
func TestValueTypeMatchesFFIDiscriminants(t *testing.T) {
	const src = `
@DATA(
  a_null    = null
  a_bool    = true
  a_int     = 42
  a_long    = 9_000_000_000L
  a_float   = 3.14f
  a_double  = 3.14159265358979
  a_string  = "hello"
  a_hex     = #FF5733
  a_array:: 1, 2, 3
  a_object: x = 1, y = 2
)`
	db, err := LoadStr(src)
	if err != nil {
		t.Fatalf("LoadStr: %v", err)
	}
	defer db.Close()

	cases := []struct {
		path string
		want ValueType
	}{
		{"a_null", TypeNull},
		{"a_bool", TypeBool},
		{"a_int", TypeInt},
		{"a_long", TypeLong},
		{"a_float", TypeFloat},
		{"a_double", TypeDouble},
		{"a_string", TypeString},
		{"a_hex", TypeHexColor},
		{"a_array", TypeArray},
		{"a_object", TypeObject},
	}
	for _, c := range cases {
		t.Run(c.path, func(t *testing.T) {
			got := db.ValueTypeAt(c.path)
			if got != c.want {
				t.Errorf("ValueTypeAt(%q) = %v (%d), want %v (%d)", c.path, got, got, c.want, c.want)
			}
		})
	}
}

func TestValueTypeString(t *testing.T) {
	cases := []struct {
		t    ValueType
		want string
	}{
		{TypeUnknown, "Unknown"},
		{TypeNull, "Null"},
		{TypeBool, "Bool"},
		{TypeInt, "Int"},
		{TypeLong, "Long"},
		{TypeFloat, "Float"},
		{TypeDouble, "Double"},
		{TypeString, "String"},
		{TypeArray, "Array"},
		{TypeObject, "Object"},
		{TypeEnum, "Enum"},
		{ValueType(99), "Unknown"},
	}
	for _, c := range cases {
		if got := c.t.String(); got != c.want {
			t.Errorf("ValueType(%d).String() = %q, want %q", c.t, got, c.want)
		}
	}
}

// TestMergeStrategyDiscriminants is the regression test for the second
// bug found in the same pass: this const block existed already
// (scaffolded ahead of merge.go) but was missing WeightedPriority
// entirely and had every other value shifted down by one from
// MdixMergeStrategy in mdix-ffi/src/lib.rs.
func TestMergeStrategyDiscriminants(t *testing.T) {
	cases := []struct {
		s    MergeStrategy
		want int32
	}{
		{WeightedPriority, 0},
		{PrimaryWins, 1},
		{SecondaryWins, 2},
		{ThrowOnConflict, 3},
	}
	for _, c := range cases {
		if int32(c.s) != c.want {
			t.Errorf("%v = %d, want %d", c.s, int32(c.s), c.want)
		}
	}
}

func TestArrayMergeStrategyDiscriminants(t *testing.T) {
	cases := []struct {
		s    ArrayMergeStrategy
		want int32
	}{
		{ArrayReplace, 0},
		{ArrayConcat, 1},
		{ArrayConcatDedup, 2},
	}
	for _, c := range cases {
		if int32(c.s) != c.want {
			t.Errorf("%v = %d, want %d", c.s, int32(c.s), c.want)
		}
	}
}
