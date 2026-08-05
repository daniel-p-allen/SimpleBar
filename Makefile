# SimpleBar — repository tasks
#
# One entry point, as in the other repos. M1–M4, M6, M7 and M9 are built —
# the producer, the window, the file watch, the wheel, the alerts and the
# remembered window geometry. Only the targets below are real. `build`, `run`
# and `install-statusline` arrive with packaging (M8): they are left out
# rather than stubbed, because a target that exists but does nothing is worse
# than one that isn't there.
#
#   statusline/  the producer binary, plus its tests
#   src-tauri/   the app's Rust side — window, file watching
#   src/         the webview UI — HTML/CSS/SVG, the wheel
#   scripts/     repository tooling (check-secrets.sh, dev-stop.sh)
#   tests/       shared fixtures and the consumer's tests

TESTS := tests

# Where `tauri build` drops the unsigned bundle. Kept in one place because both
# `build` (asserts it appeared) and `run` (opens it) name it.
# Universal builds land under their target triple, not plain `release/`.
APP := src-tauri/target/universal-apple-darwin/release/bundle/macos/SimpleBar.app
DMG_DIR := src-tauri/target/universal-apple-darwin/release/bundle/dmg

.PHONY: help test test-producer test-app check clean dev-stop build producer run install-statusline

# Default target: say what you can do, rather than doing something surprising.
help:
	@echo "make build   — build both binaries and the unsigned app bundle"
	@echo "make run     — build if needed, then open the app"
	@echo "make install-statusline — wire the producer into ~/.claude/settings.json"
	@echo "make test    — run the producer's tests"
	@echo "make check   — refuse to ship a committed credential"
	@echo "make dev-stop — stop the dev app and everything it started"
	@echo "make clean   — remove local build and test artefacts"

# Release build of both halves. The producer (and the install helper beside it)
# come from the statusline crate; the app bundle comes from `tauri build`. This
# is an unsigned local build by decision — no Apple Developer account, so no
# signing or notarization flags. `npm install` runs first because `tauri build`
# needs the CLI, and a fresh clone will not have it yet.
build: producer
	npm install
	npx tauri build --target universal-apple-darwin

# The producer, in release, as a universal binary.
#
# A prerequisite of anything that touches the app rather than a step inside
# `build`, because the app cannot be compiled without it: tauri.conf.json lists
# the binary as a bundled resource, and tauri-build fails outright — "resource
# path ... doesn't exist" — when it is missing. That is a build-time dependency
# between the two crates, so it is expressed as one.
#
# Built for both architectures and merged with `lipo`, because a single
# download that runs everywhere beats asking a user which processor their Mac
# has — a question many cannot answer, and getting it wrong looks like a broken
# app rather than a wrong choice. The cost is a few megabytes.
#
# The installer is merged too, and both land in `target/release/` rather than a
# per-architecture directory: `simplebar-install` finds the producer as its own
# sibling, so the two must sit together, and `tauri.conf.json` names that path.
producer:
	rustup target add x86_64-apple-darwin aarch64-apple-darwin
	cargo build --release --manifest-path statusline/Cargo.toml --target x86_64-apple-darwin
	cargo build --release --manifest-path statusline/Cargo.toml --target aarch64-apple-darwin
	@mkdir -p statusline/target/release
	lipo -create -output statusline/target/release/simplebar-statusline \
		statusline/target/x86_64-apple-darwin/release/simplebar-statusline \
		statusline/target/aarch64-apple-darwin/release/simplebar-statusline
	lipo -create -output statusline/target/release/simplebar-install \
		statusline/target/x86_64-apple-darwin/release/simplebar-install \
		statusline/target/aarch64-apple-darwin/release/simplebar-install

# Open the built app, building first only if the bundle is absent — so a plain
# `make run` after a build is instant, but a fresh clone still just works. A
# shell test rather than a prerequisite because `build` is phony and would
# otherwise force a rebuild on every run.
run:
	@test -d "$(APP)" || $(MAKE) build
	open "$(APP)"

# Wire the producer into ~/.claude/settings.json. The installer is a sibling
# binary built alongside the producer, so it needs the release build first;
# building only the statusline crate rather than the whole app keeps it quick
# when the user only wants the status line, not the window.
install-statusline: producer
	./statusline/target/release/simplebar-install

# The producer runs inside Claude Code's status line, in the critical path of
# every prompt render. These tests exist to prove it stays quiet and exits 0 on
# malformed input, not just that the happy path works.
#
# Everything, for local use. Split into halves below because they need
# different things installed: `test-app` builds the Tauri crate, which drags in
# the system webview and GTK, while `test-producer` needs only cargo and node.
# CI runs them separately for that reason — the Linux job cannot build the app
# and should not have to.
test: test-producer test-app

# The producer that ships in your status line, plus the consumer's pure
# functions. The two Rust crates are separate cargo projects, so neither
# `cargo test` finds the other's tests.
test-producer:
	@cd statusline && cargo test --quiet
	@node $(TESTS)/test_format.mjs
	@node $(TESTS)/test_alerts.mjs
	@node $(TESTS)/test_staleness.mjs
	@node $(TESTS)/test_selectors.mjs

# The app's own Rust side — config handling. Needs the Tauri build
# dependencies, so it belongs wherever the app is already being built.
test-app: producer
	@cd src-tauri && cargo test --quiet

# Refuse to ship if anything resembling a credential is in the tree. Stage 1
# holds no secrets by design, so this guards against accident — a token pasted
# into a fixture, or someone's real settings.json committed by mistake.
#
# Kept separate from `test`: this needs nothing installed and runs in a second,
# which is what makes it usable as a habit before every push.
check:
	@./scripts/check-secrets.sh

# `npx tauri dev` runs four processes deep, so stopping it by hand reliably
# leaves orphans behind — quiet ones, which then confuse the next check into
# reporting instances that aren't running.
dev-stop:
	@./scripts/dev-stop.sh

clean:
	rm -rf __pycache__ $(TESTS)/__pycache__ target src-tauri/target dist
