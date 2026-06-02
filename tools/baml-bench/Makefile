# baml-bench test tiers. Mirrors what CI runs (.github/workflows/baml-bench.yml).
# By default the integration + system tiers run against the in-process MemoryGateway:
# no Convex, no Docker, seconds to run. `test-real` re-runs them against the real
# Convex backend (needs Docker + Node) for fidelity.
#   unit         - fast logic / TestClient / fakes
#   integration  - per-hop pipeline tests        (MemoryGateway by default)
#   system       - full bug -> fix flow          (MemoryGateway by default)
#   stack        - integration + system together
#   test         - the whole suite
#   test-real    - integration + system vs real Convex (Docker + Node)
.PHONY: test test-unit test-integration test-system test-stack test-real

test-unit:
	uv run --extra dev pytest -m "not integration and not system" -q

test-integration:
	uv run --extra dev pytest -m "integration" -q

test-system:
	uv run --extra dev pytest -m "system" -q

test-stack:
	uv run --extra dev pytest -m "integration or system" -q

test:
	uv run --extra dev pytest -q

test-real:
	BAML_BENCH_REAL_CONVEX=1 BAML_BENCH_REQUIRE_DOCKER=1 uv run --extra dev pytest -m "integration or system" -q
