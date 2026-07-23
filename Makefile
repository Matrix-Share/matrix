# Project Lifeline — common developer tasks.
.PHONY: help build test fmt fmt-check clippy sim run relay node up down docker clean

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN{FS=":.*?## "}{printf "  \033[36m%-12s\033[0m %s\n",$$1,$$2}'

build: ## Build the whole workspace
	cargo build --workspace

test: ## Run all tests
	cargo test --workspace

fmt: ## Format the code
	cargo fmt --all

fmt-check: ## Check formatting (CI)
	cargo fmt --all -- --check

clippy: ## Lint with clippy, warnings as errors
	cargo clippy --all-targets -- -D warnings

sim: ## Run the acceptance simulator
	cargo run -p lifeline-sim --release

relay: ## Run a local relay hub on :7000
	cargo run -p lifeline-relay

node: ## Run a local node GUI on :8080 (needs a relay)
	LIFELINE_NODE_ADDR=127.0.0.1:8080 cargo run -p lifeline-node

up: ## Start the full demo mesh via Docker (relay + two nodes)
	docker compose up --build

down: ## Stop the Docker demo mesh
	docker compose down

docker: ## Build the Docker image
	docker build -t lifeline .

clean: ## Remove build artifacts
	cargo clean
