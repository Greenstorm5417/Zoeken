ASSETS_DIR := zoeken/zoeken-server/assets
VERSION ?= $(shell ./packaging/scripts/package-version.sh)
OUT_DIR ?= dist

.PHONY: help clean-assets build client package deb deb-amd64 deb-arm64 docker native-types check-native-types sync-versions

help:
	@echo "make targets:"
	@echo "  client            bun install + build zoeken-client into assets"
	@echo "  build             client + cargo build --release --bin zoeken-server"
	@echo "  package           build and copy assets beside the release binary"
	@echo "  deb               build .deb for host architecture (needs dpkg-deb)"
	@echo "  deb-amd64         build amd64 .deb (native x86_64 host)"
	@echo "  deb-arm64         build arm64 .deb (native aarch64 host)"
	@echo "  docker            docker build -t zoeken:local ."
	@echo "  native-types      regenerate SPA types from Rust wire DTOs"
	@echo "  check-native-types fail if generated native.ts drifts"
	@echo "  sync-versions     sync package.json / lock / Docker VERSION to Cargo.toml"
	@echo "                    (optional BUMP=X.Y.Z; CHECK=1 for drift gate)"
	@echo "  clean-assets      Remove built assets, keeping .gitkeep / rss.xsl / logo"

native-types:     ## regenerate SPA types from Rust wire DTOs
	cargo run --locked -p zoeken-server --bin export-native-ts

check-native-types: native-types
	git diff --exit-code -- zoeken-client/src/lib/generated/native.ts

# Sync package.json, Cargo.lock zoeken-*, and Docker VERSION defaults to Cargo.toml.
# Examples: make sync-versions | make sync-versions BUMP=1.4.0 | make sync-versions CHECK=1
sync-versions:
	@chmod +x tools/sync_versions.sh
	@args=""; \
	  if [ -n "$(BUMP)" ]; then args="$$args --bump $(BUMP)"; fi; \
	  if [ "$(CHECK)" = "1" ]; then args="$$args --check"; fi; \
	  if [ "$(DRY_RUN)" = "1" ]; then args="$$args --dry-run"; fi; \
	  ./tools/sync_versions.sh $$args

clean-assets:
	@find $(ASSETS_DIR) -mindepth 1 ! -name '.gitkeep' ! -name 'rss.xsl' ! -name 'zoeken-logo.svg' -exec rm -rf {} +
	@echo "cleaned $(ASSETS_DIR)"

client:
	cd zoeken-client && bun install && bun run build
	@cp -f logo/zoeken-logo.svg $(ASSETS_DIR)/zoeken-logo.svg 2>/dev/null || true
	@if [ ! -f $(ASSETS_DIR)/rss.xsl ]; then \
	  printf '%s\n' '<?xml version="1.0" encoding="UTF-8"?>' \
	    '<xsl:stylesheet version="1.0" xmlns:xsl="http://www.w3.org/1999/XSL/Transform">' \
	    '  <xsl:output method="html"/>' \
	    '  <xsl:template match="/"><html><body><xsl:apply-templates/></body></html></xsl:template>' \
	    '</xsl:stylesheet>' > $(ASSETS_DIR)/rss.xsl; \
	fi
	@test -f $(ASSETS_DIR)/index.html

build: client
	cargo build --release --locked --bin zoeken-server

package: build
	mkdir -p target/release/assets
	cp -R $(ASSETS_DIR)/. target/release/assets/
	@echo "packaged: target/release/zoeken-server + target/release/assets/"

deb: package
	chmod +x packaging/scripts/*.sh
	BINARY=target/release/zoeken-server ASSETS=$(ASSETS_DIR) \
	  VERSION=$(VERSION) OUT_DIR=$(OUT_DIR) ./packaging/scripts/build-deb.sh

deb-amd64: client
	chmod +x packaging/scripts/*.sh
	cargo build --release --locked --bin zoeken-server --target x86_64-unknown-linux-gnu
	BINARY=target/x86_64-unknown-linux-gnu/release/zoeken-server ASSETS=$(ASSETS_DIR) \
	  VERSION=$(VERSION) ARCH=amd64 OUT_DIR=$(OUT_DIR) ./packaging/scripts/build-deb.sh

deb-arm64: client
	chmod +x packaging/scripts/*.sh
	cargo build --release --locked --bin zoeken-server --target aarch64-unknown-linux-gnu
	BINARY=target/aarch64-unknown-linux-gnu/release/zoeken-server ASSETS=$(ASSETS_DIR) \
	  VERSION=$(VERSION) ARCH=arm64 OUT_DIR=$(OUT_DIR) ./packaging/scripts/build-deb.sh

docker:
	docker build -t zoeken:local \
	  --build-arg VERSION=$(VERSION) \
	  --build-arg REVISION=$$(git rev-parse HEAD 2>/dev/null || echo unknown) \
	  .
