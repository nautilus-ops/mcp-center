CONFIG ?= "bootstrap.toml"
LOG_LEVEL ?= "info"

start-local:
	@RUST_LOG=$(LOG_LEVEL) cargo run -- run --config $(CONFIG)