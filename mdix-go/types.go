package dixscript

import (
	"encoding/base64"
	"fmt"
	"regexp"
	"strconv"
	"sync"
	"time"
)

// ValueType identifies the DixScript type of a value at a given path.
type ValueType int

const (
	TypeUnknown   ValueType = -1
	TypeNull      ValueType = 0
	TypeBool      ValueType = 1
	TypeInt       ValueType = 2
	TypeFloat     ValueType = 3
	TypeDouble    ValueType = 4
	TypeString    ValueType = 5
	TypeDate      ValueType = 6
	TypeTimestamp ValueType = 7
	TypeHexColor  ValueType = 8
	TypeBlob      ValueType = 9
	TypeRegex     ValueType = 10
	TypeArray     ValueType = 11
	TypeObject    ValueType = 12
	TypeTuple     ValueType = 13
	TypeEnum      ValueType = 14
)

func (t ValueType) String() string {
	switch t {
	case TypeNull:
		return "Null"
	case TypeBool:
		return "Bool"
	case TypeInt:
		return "Int"
	case TypeFloat:
		return "Float"
	case TypeDouble:
		return "Double"
	case TypeString:
		return "String"
	case TypeDate:
		return "Date"
	case TypeTimestamp:
		return "Timestamp"
	case TypeHexColor:
		return "HexColor"
	case TypeBlob:
		return "Blob"
	case TypeRegex:
		return "Regex"
	case TypeArray:
		return "Array"
	case TypeObject:
		return "Object"
	case TypeTuple:
		return "Tuple"
	case TypeEnum:
		return "Enum"
	default:
		return "Unknown"
	}
}

// ── HexColor ─────────────────────────────────────────────────────────────────

// HexColor is a color parsed from a DixScript hex literal (e.g. #FF5733).
// Channel values are in the 0–1 range.
type HexColor struct {
	Raw        string  // original hex string, e.g. "#FF5733"
	R, G, B, A float32 // channels, 0–1
}

// ParseHexColor parses a hex color string (#RGB, #RRGGBB, or #RRGGBBAA).
func ParseHexColor(raw string) (HexColor, error) {
	s := raw
	if len(s) > 0 && s[0] == '#' {
		s = s[1:]
	}

	hexByte := func(s string, offset int) (float32, error) {
		n, err := strconv.ParseUint(s[offset:offset+2], 16, 8)
		if err != nil {
			return 0, err
		}
		return float32(n) / 255.0, nil
	}

	hexNibble := func(c byte) (float32, error) {
		n, err := strconv.ParseUint(string(c), 16, 4)
		if err != nil {
			return 0, err
		}
		return float32(n) / 15.0, nil
	}

	switch len(s) {
	case 3:
		r, err := hexNibble(s[0])
		if err != nil {
			return HexColor{}, fmt.Errorf("invalid hex color %q: %w", raw, err)
		}
		g, err := hexNibble(s[1])
		if err != nil {
			return HexColor{}, fmt.Errorf("invalid hex color %q: %w", raw, err)
		}
		b, err := hexNibble(s[2])
		if err != nil {
			return HexColor{}, fmt.Errorf("invalid hex color %q: %w", raw, err)
		}
		return HexColor{Raw: raw, R: r, G: g, B: b, A: 1.0}, nil

	case 6:
		r, _ := hexByte(s, 0)
		g, _ := hexByte(s, 2)
		b, _ := hexByte(s, 4)
		return HexColor{Raw: raw, R: r, G: g, B: b, A: 1.0}, nil

	case 8:
		r, _ := hexByte(s, 0)
		g, _ := hexByte(s, 2)
		b, _ := hexByte(s, 4)
		a, _ := hexByte(s, 6)
		return HexColor{Raw: raw, R: r, G: g, B: b, A: a}, nil

	default:
		return HexColor{}, fmt.Errorf("invalid hex color length in %q (expected #RGB, #RRGGBB, or #RRGGBBAA)", raw)
	}
}

func (h HexColor) String() string { return h.Raw }

// ── Blob ─────────────────────────────────────────────────────────────────────

// Blob holds base64-encoded binary data from a DixScript b:("...") literal.
// Call Bytes() to decode.
type Blob struct {
	RawBase64 string
}

// Bytes decodes the base64 content and returns the raw bytes.
func (b Blob) Bytes() ([]byte, error) {
	return base64.StdEncoding.DecodeString(b.RawBase64)
}

// DecodedSize returns the approximate decoded byte count without allocating.
func (b Blob) DecodedSize() int {
	n := len(b.RawBase64)
	if n == 0 {
		return 0
	}
	padding := 0
	if n >= 1 && b.RawBase64[n-1] == '=' {
		padding++
	}
	if n >= 2 && b.RawBase64[n-2] == '=' {
		padding++
	}
	return (n/4)*3 - padding
}

func (b Blob) String() string { return fmt.Sprintf("b:(%q)", b.RawBase64) }

// ── Regex ────────────────────────────────────────────────────────────────────

// MdixRegex holds a regular expression pattern from a DixScript r:("...") literal.
// Compiled patterns are cached.
type MdixRegex struct {
	Pattern string
}

var (
	regexCacheMu sync.Mutex
	regexCache   = map[string]*regexp.Regexp{}
)

// Compile returns a compiled *regexp.Regexp, cached by pattern string.
func (r MdixRegex) Compile() (*regexp.Regexp, error) {
	regexCacheMu.Lock()
	defer regexCacheMu.Unlock()
	if re, ok := regexCache[r.Pattern]; ok {
		return re, nil
	}
	re, err := regexp.Compile(r.Pattern)
	if err != nil {
		return nil, err
	}
	regexCache[r.Pattern] = re
	return re, nil
}

// MatchString reports whether the pattern matches s.
func (r MdixRegex) MatchString(s string) (bool, error) {
	re, err := r.Compile()
	if err != nil {
		return false, err
	}
	return re.MatchString(s), nil
}

func (r MdixRegex) String() string { return fmt.Sprintf("r:(%q)", r.Pattern) }

// ── Date ──────────────────────────────────────────────────────────────────────

// MdixDate is a date value (YYYY-MM-DD) from DixScript.
type MdixDate struct {
	Value   time.Time
	RawStr  string
}

// ParseMdixDate parses a YYYY-MM-DD string.
func ParseMdixDate(raw string) (MdixDate, error) {
	t, err := time.ParseInLocation("2006-01-02", raw, time.UTC)
	if err != nil {
		return MdixDate{}, fmt.Errorf("invalid date %q: %w", raw, err)
	}
	return MdixDate{Value: t, RawStr: raw}, nil
}

func (d MdixDate) String() string { return d.RawStr }

// ── Timestamp ─────────────────────────────────────────────────────────────────

// MdixTimestamp is an ISO 8601 timestamp from DixScript.
type MdixTimestamp struct {
	Value  time.Time
	RawStr string
}

// ParseMdixTimestamp parses an ISO 8601 string.
func ParseMdixTimestamp(raw string) (MdixTimestamp, error) {
	t, err := time.Parse(time.RFC3339Nano, raw)
	if err != nil {
		// Try without nanoseconds
		t, err = time.Parse(time.RFC3339, raw)
		if err != nil {
			return MdixTimestamp{}, fmt.Errorf("invalid timestamp %q: %w", raw, err)
		}
	}
	return MdixTimestamp{Value: t, RawStr: raw}, nil
}

func (ts MdixTimestamp) String() string { return ts.RawStr }

// ── FormatMode ────────────────────────────────────────────────────────────────

// FormatMode controls how DixScript source or database content is formatted.
type FormatMode int32

const (
	FormatDefault  FormatMode = 0 // readable, 2-space indent
	FormatPretty   FormatMode = 1 // readable, 4-space indent, sorted keys
	FormatCompact  FormatMode = 2 // no trailing whitespace, collapsed blank lines
	FormatMinified FormatMode = 3 // smallest possible output
)

// MergeStrategy controls conflict resolution when merging two databases.
type MergeStrategy int

const (
	PrimaryWins     MergeStrategy = iota // primary keys take precedence (default)
	SecondaryWins                        // secondary keys overwrite primary
	ThrowOnConflict                      // any conflict returns an error
)
