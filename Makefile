# SimpleBar — repository tasks
#
# One entry point, as in the other repos. The project is at M1, so only the
# producer exists and only its targets are real. `build` and `run` arrive with
# the Tauri app at M2; `install-statusline` with the installer milestone. They
# are left out rather than stubbed, because a target that exists but does
# nothing is worse than one that isn't there.
#
#   scripts/   the producer, plus repository tooling
#   tests/     fixtures and tests for the producer
#   docs/

TESTS := tests

.PHONY: help test test-producer test-app check clean dev-stop

# Default target: say what you can do, rather than doing something surprising.
help:
	@echo "make test    — run the producer's tests"
	@echo "make check   — refuse to ship a committed credential"
	@echo "make dev-stop — stop the dev app and everything it started"
	@echo "make clean   — remove local build and test artefacts"

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
test-app:
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
