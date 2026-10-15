# Claudimon — convenience targets. `make && sudo make install` installs system-wide.
PREFIX  ?= /usr/local
BINDIR  := $(DESTDIR)$(PREFIX)/bin
DOCDIR  := $(DESTDIR)$(PREFIX)/share/doc/claudimon
BIN     := target/release/claudimon

.PHONY: all build run test install uninstall screenshots clean

all: build

build:
	cargo build --release --locked

run:
	cargo run --release --locked

test:
	cargo test --locked

# Doesn't rebuild, so `sudo make install` never runs cargo as root.
install:
	@test -x $(BIN) || { echo "Run 'make' first (as your normal user)."; exit 1; }
	install -Dm755 $(BIN) $(BINDIR)/claudimon
	install -Dm644 README.md $(DOCDIR)/README.md
	install -Dm644 LICENSE $(DOCDIR)/LICENSE

uninstall:
	rm -f $(BINDIR)/claudimon
	rm -rf $(DOCDIR)

screenshots:
	./tools/screenshots.sh

clean:
	cargo clean
