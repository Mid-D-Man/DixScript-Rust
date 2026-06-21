package main

// examples/hello.odin — run with: odin run examples/hello.odin -file
// (or place this in its own package dir and `odin run examples`).
// Requires libmdix_ffi to be discoverable by the linker — see ../README.md.

import "core:fmt"
import mdix "../mdix"

main :: proc() {
	defer free_all(context.temp_allocator)

	db, ok := mdix.load_str(`@DATA( port = 8080, host = "localhost", ssl = true )`)
	if !ok {
		fmt.println("load failed:", mdix.last_error())
		return
	}
	defer mdix.destroy(&db)

	host, host_ok := mdix.get_string(db, "host")
	defer if host_ok { delete(host) }
	port, _ := mdix.get_int(db, "port")
	ssl, _ := mdix.get_bool(db, "ssl")

	fmt.printf("%s:%d (ssl=%v)\n", host, port, ssl)

	// Builder round-trip
	b := mdix.builder_new()
	defer mdix.builder_destroy(&b)

	mdix.builder_set_string(b, "app", "MyGame")
	mdix.builder_set_int(b, "port", 9000)
	mdix.builder_set_bool(b, "ssl", true)

	out, out_ok := mdix.builder_to_string(b)
	defer if out_ok { delete(out) }
	if out_ok {
		fmt.println(out)
	}
}
