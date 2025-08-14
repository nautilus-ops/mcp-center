CONFIG ?= "bootstrap.toml"
LOG_LEVEL ?= "info"
IMAGE_REGISTER ?= ""
PLATFORM ?= linux/amd64,linux/arm64
RELEASE ?= false

start-local:
	@RUST_LOG=$(LOG_LEVEL) cargo run -p mc-service -- run --config $(CONFIG)


# Build local Docker image
build-image:
	@echo "Building local Docker image mcp-center:latest"
	@docker buildx build \
		--platform $(PLATFORM) \
		--tag mcp-center:latest \
		.

# Build and push Docker image to remote registry
build-push-image:
	@if [ -z "$(IMAGE_REGISTER)" ]; then \
		echo "Error: Please set IMAGE_REGISTER environment variable"; \
		echo "Example: make build-push-image IMAGE_REGISTER=your-registry.com/your-namespace"; \
		exit 1; \
	fi
	@echo "Building and pushing Docker image to $(IMAGE_REGISTER)/mcp-center"
	@docker buildx build \
		--platform $(PLATFORM) \
		--tag $(IMAGE_REGISTER)/mcp-center:$(shell grep '^version = ' Cargo.toml | cut -d'"' -f2)-$(shell date +%Y%m%d%H%M)-$(shell git rev-parse --short HEAD 2>/dev/null || echo "unknown") \
		$(if $(filter true,$(RELEASE)),--tag $(IMAGE_REGISTER)/mcp-center:latest) \
		$(if $(filter true,$(RELEASE)),--tag $(IMAGE_REGISTER)/mcp-center:$(shell grep '^version = ' Cargo.toml | cut -d'"' -f2)-release) \
		--push \
		.

# Run local Docker container
run-docker:
	@echo "Running local Docker container"
	@docker run --rm -p 8080:8080 \
		-v $(PWD)/bootstrap.toml:/app/bootstrap.toml \
		-v $(PWD)/mcp_servers.toml.example:/app/mcp_servers.toml \
		mcp-center:latest

# Clean local Docker image
clean-image:
	@echo "Cleaning local Docker image"
	@docker rmi mcp-center:latest 2>/dev/null || true