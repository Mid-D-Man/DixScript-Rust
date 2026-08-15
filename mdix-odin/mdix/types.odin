package mdix

// types.odin — typed convenience wrappers for values whose canonical
// mdix-ffi representation is a plain string (mdix_get_string). None of
// these need new FFI surface — same reasoning as the Go binding's
// GetHexColor/GetBlob/GetRegex/GetDate/GetTimestamp (mdix-go/database.go):
// the string *is* the value at the FFI layer, these just parse it into
// something more useful than a bare Odin string.

import "core:encoding/base64"
import "core:strconv"
import "core:strings"
import "core:text/regex"
import "core:time"

// ── Hex_Color ────────────────────────────────────────────────────────────

Hex_Color :: struct {
	raw:        string, // original hex string, e.g. "#FF5733"
	r, g, b, a: f32,    // channels, 0–1
}

// parse_hex_color parses #RGB, #RRGGBB, or #RRGGBBAA.
parse_hex_color :: proc(raw: string) -> (Hex_Color, bool) {
	s := raw
	if len(s) > 0 && s[0] == '#' {
		s = s[1:]
	}

	hex_nibble :: proc(c: u8) -> (f32, bool) {
		v: int
		switch c {
		case '0' ..= '9': v = int(c - '0')
		case 'a' ..= 'f': v = int(c - 'a') + 10
		case 'A' ..= 'F': v = int(c - 'A') + 10
		case: return 0, false
		}
		// Single nibble expands to a full byte the same way "#RGB" CSS
		// shorthand does: 0xF -> 0xFF, not 0x0F.
		return f32(v * 16 + v) / 255.0, true
	}

	hex_byte :: proc(s: string, offset: int) -> (f32, bool) {
		hi, ok1 := strconv.parse_uint(s[offset:offset + 1], 16)
		lo, ok2 := strconv.parse_uint(s[offset + 1:offset + 2], 16)
		if !ok1 || !ok2 {
			return 0, false
		}
		return f32(hi * 16 + lo) / 255.0, true
	}

	switch len(s) {
	case 3: // RGB
		r, ok1 := hex_nibble(s[0])
		g, ok2 := hex_nibble(s[1])
		b, ok3 := hex_nibble(s[2])
		if !ok1 || !ok2 || !ok3 {
			return {}, false
		}
		return Hex_Color{raw = raw, r = r, g = g, b = b, a = 1}, true
	case 6: // RRGGBB
		r, ok1 := hex_byte(s, 0)
		g, ok2 := hex_byte(s, 2)
		b, ok3 := hex_byte(s, 4)
		if !ok1 || !ok2 || !ok3 {
			return {}, false
		}
		return Hex_Color{raw = raw, r = r, g = g, b = b, a = 1}, true
	case 8: // RRGGBBAA
		r, ok1 := hex_byte(s, 0)
		g, ok2 := hex_byte(s, 2)
		b, ok3 := hex_byte(s, 4)
		a, ok4 := hex_byte(s, 6)
		if !ok1 || !ok2 || !ok3 || !ok4 {
			return {}, false
		}
		return Hex_Color{raw = raw, r = r, g = g, b = b, a = a}, true
	case:
		return {}, false
	}
}

get_hex_color :: proc(db: Database, path: string) -> (Hex_Color, bool) {
	raw, ok := get_string(db, path, context.temp_allocator)
	if !ok {
		return {}, false
	}
	return parse_hex_color(raw)
}

// ── Blob ─────────────────────────────────────────────────────────────────

Blob :: struct {
	raw_base64: string,
}

// blob_bytes decodes the base64 content into raw bytes, allocated with
// `allocator` and owned by the caller.
blob_bytes :: proc(b: Blob, allocator := context.allocator) -> ([]byte, bool) {
	decoded, err := base64.decode(b.raw_base64, allocator = allocator)
	return decoded, err == nil
}

get_blob :: proc(db: Database, path: string, allocator := context.allocator) -> (Blob, bool) {
	raw, ok := get_string(db, path, allocator)
	if !ok {
		return {}, false
	}
	return Blob{raw_base64 = raw}, true
}

// ── Regex ────────────────────────────────────────────────────────────────

Mdix_Regex :: struct {
	pattern: string,
}

// regex_compile compiles the pattern via core:text/regex. Not cached
// (unlike the Go binding's Compile, which caches by pattern string) —
// Odin's Regular_Expression owns allocator-backed state the caller
// controls the lifetime of via `allocator`; cache it yourself at the
// call site if you're compiling the same pattern repeatedly.
regex_compile :: proc(r: Mdix_Regex, allocator := context.allocator) -> (regex.Regular_Expression, bool) {
	re, err := regex.create(r.pattern, permanent_allocator = allocator)
	return re, err == nil
}

get_regex :: proc(db: Database, path: string, allocator := context.allocator) -> (Mdix_Regex, bool) {
	raw, ok := get_string(db, path, allocator)
	if !ok {
		return {}, false
	}
	return Mdix_Regex{pattern = raw}, true
}

// ── Date ─────────────────────────────────────────────────────────────────

Mdix_Date :: struct {
	value: time.Time,
	raw:   string,
}

// parse_mdix_date parses a YYYY-MM-DD string. Manual field-by-field
// parsing rather than a format-string layout engine (Odin's core:time
// doesn't have Go-style reference-time layouts) — DixScript's date
// format is fixed and simple enough that this is more direct than
// reaching for a general parser.
parse_mdix_date :: proc(raw: string) -> (Mdix_Date, bool) {
	if len(raw) != 10 || raw[4] != '-' || raw[7] != '-' {
		return {}, false
	}
	year, ok1 := strconv.parse_int(raw[0:4])
	month, ok2 := strconv.parse_int(raw[5:7])
	day, ok3 := strconv.parse_int(raw[8:10])
	if !ok1 || !ok2 || !ok3 || month < 1 || month > 12 || day < 1 || day > 31 {
		return {}, false
	}
	t, ok := time.datetime_to_time(year, month, day, 0, 0, 0)
	if !ok {
		return {}, false
	}
	return Mdix_Date{value = t, raw = raw}, true
}

get_date :: proc(db: Database, path: string) -> (Mdix_Date, bool) {
	raw, ok := get_string(db, path, context.temp_allocator)
	if !ok {
		return {}, false
	}
	return parse_mdix_date(raw)
}

// ── Timestamp ────────────────────────────────────────────────────────────

Mdix_Timestamp :: struct {
	value: time.Time,
	raw:   string,
}

// parse_mdix_timestamp parses an ISO 8601 / RFC 3339 string:
// YYYY-MM-DDTHH:MM:SS[.fraction][Z]. Same manual-parsing rationale as
// parse_mdix_date. A non-"Z" numeric UTC offset (+HH:MM) is intentionally
// not supported — DixScript's own serializer (dixscript/src/Runtime/
// dix_serialize.rs) always emits "Z", so this only needs to round-trip
// what this project itself produces.
parse_mdix_timestamp :: proc(raw: string) -> (Mdix_Timestamp, bool) {
	if len(raw) < 19 || raw[4] != '-' || raw[7] != '-' || raw[10] != 'T' ||
	   raw[13] != ':' || raw[16] != ':' {
		return {}, false
	}
	year, ok1 := strconv.parse_int(raw[0:4])
	month, ok2 := strconv.parse_int(raw[5:7])
	day, ok3 := strconv.parse_int(raw[8:10])
	hour, ok4 := strconv.parse_int(raw[11:13])
	minute, ok5 := strconv.parse_int(raw[14:16])
	second, ok6 := strconv.parse_int(raw[17:19])
	if !ok1 || !ok2 || !ok3 || !ok4 || !ok5 || !ok6 {
		return {}, false
	}
	if month < 1 || month > 12 || day < 1 || day > 31 || hour > 23 || minute > 59 || second > 60 {
		return {}, false
	}

	nanos := 0
	rest := raw[19:]
	if len(rest) > 0 && rest[0] == '.' {
		frac_end := 1
		for frac_end < len(rest) && rest[frac_end] >= '0' && rest[frac_end] <= '9' {
			frac_end += 1
		}
		frac_str := rest[1:frac_end]
		digit_count := len(frac_str)
		if digit_count > 0 {
			if n, ok := strconv.parse_int(frac_str); ok {
				// Pad/truncate to exactly 9 digits (nanosecond precision):
				// ".123" (millis) -> 123_000_000 ns, ".123456789012" -> truncated to 9 digits.
				padded := n
				for digit_count < 9 {
					padded *= 10
					digit_count += 1
				}
				for digit_count > 9 {
					padded /= 10
					digit_count -= 1
				}
				nanos = padded
			}
		}
	}

	t, ok := time.datetime_to_time(year, month, day, hour, minute, second, nanos)
	if !ok {
		return {}, false
	}
	return Mdix_Timestamp{value = t, raw = raw}, true
}

get_timestamp :: proc(db: Database, path: string) -> (Mdix_Timestamp, bool) {
	raw, ok := get_string(db, path, context.temp_allocator)
	if !ok {
		return {}, false
	}
	return parse_mdix_timestamp(raw)
}

// ── Enum ─────────────────────────────────────────────────────────────────

// get_enum_value returns an enum path's resolved integer value —
// mdix_get_int already works on Enum paths directly (see its doc comment
// in mdix-ffi/src/lib.rs), so this is just a clearer name for that case.
get_enum_value :: proc(db: Database, path: string) -> (int, bool) {
	return get_int(db, path)
}
