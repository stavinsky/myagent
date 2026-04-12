.PHONY: all install uninstall build test clean setup-config help

# Default target
all: help

# Install binary and setup configuration
install: build setup-config
	@echo "Installation complete!"
	@echo "  Binary: ~/.cargo/bin/myagent"
	@echo "  Config: ~/.config/myagent/config.yaml"

# Build and install binary
build:
	@echo "Building myagent..."
	cargo build --release
	@echo "Installing to ~/.cargo/bin..."
	cargo install --path .

# Setup configuration directory and file
setup-config:
	@bash -c 'if [ -f ~/.config/myagent/config.yaml ]; then \
		echo "Config exists at ~/.config/myagent/config.yaml"; \
		echo "  Skipping to preserve existing config."; \
	else \
		mkdir -p ~/.config/myagent; \
		if [ -f config.example.yaml ]; then \
			cp config.example.yaml ~/.config/myagent/config.yaml; \
			echo "Created default config"; \
		else \
			echo "config.example.yaml not found"; \
		fi; \
	fi'

# Uninstall binary and optionally config
uninstall:
	@echo "Uninstalling myagent..."
	@rm -f ~/.cargo/bin/myagent
	@echo "Binary removed"
	@echo ""
	@echo "To remove config: rm -rf ~/.config/myagent"

# Run tests
test:
	@echo "Running tests..."
	cargo test

# Check code with clippy
check:
	@echo "Running clippy..."
	cargo clippy -- -D warnings

# Format code
fmt:
	@echo "Formatting code..."
	cargo fmt

# Clean build artifacts
clean:
	@echo "Cleaning..."
	cargo clean

# Show help
help:
	@echo "myagent - Makefile"
	@echo ""
	@echo "  make install     Build and install binary + setup config"
	@echo "  make build       Build and install binary only"
	@echo "  make setup-config Create config directory and default config"
	@echo "  make uninstall   Remove binary from ~/.cargo/bin"
	@echo "  make test        Run tests"
	@echo "  make check       Run clippy"
	@echo "  make fmt         Format code"
	@echo "  make clean       Remove build artifacts"
	@echo "  make help        Show this help"
