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

.PHONY: help test check clean dev-stop

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
# Two suites, one per half: the Rust producer that ships in your status line,
# and the consumer's pure functions. Both cover code that actually runs — the
# Python prototype and its tests were retired once the Rust suite covered
# everything they did.
test:
	@cd statusline && cargo test --quiet
	@node $(TESTS)/test_format.mjs
	@node $(TESTS)/test_alerts.mjs
	@node $(TESTS)/test_selectors.mjs

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
