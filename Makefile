# Release builds. See README "Building" for prerequisites.
GUI_TARGET    := x86_64-unknown-linux-gnu
STATIC_TARGET := x86_64-unknown-linux-musl

.PHONY: build-gui build-static test check-gui check-static

# GUI + CLI + text menu, linked against glibc 2.17 so it runs on almost any desktop Linux.
build-gui:
	cargo zigbuild --release --target $(GUI_TARGET).2.17
	@echo target/$(GUI_TARGET)/release/playlistgenrs

# CLI + text menu only, fully static (no GUI: egui needs libGL/X11 via dlopen).
build-static:
	cargo build --release --no-default-features --target $(STATIC_TARGET)
	@echo target/$(STATIC_TARGET)/release/playlistgenrs

test:
	cargo test
	cargo test --no-default-features
	cargo clippy --all-targets -- -D warnings
	cargo fmt --check

check-gui:
	file target/$(GUI_TARGET)/release/playlistgenrs
	ldd target/$(GUI_TARGET)/release/playlistgenrs
	objdump -T target/$(GUI_TARGET)/release/playlistgenrs | grep -o 'GLIBC_[0-9.]*' | sort -uV | tail -1

check-static:
	file target/$(STATIC_TARGET)/release/playlistgenrs
	ldd target/$(STATIC_TARGET)/release/playlistgenrs || true
