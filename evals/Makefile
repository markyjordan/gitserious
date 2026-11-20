.PHONY: setup lint format-check test smoke view

setup:
	uv sync --python 3.12 --locked

lint:
	uv run --locked ruff check .

format-check:
	uv run --locked ruff format --check .

test:
	uv run --locked pytest -q

smoke:
	uv run --locked --offline pytest -q tests/test_smoke.py

view:
	uv run --locked inspect view --log-dir artifacts/logs
