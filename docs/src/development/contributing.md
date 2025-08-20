# Contributing

Thank you for your interest in contributing to kasl! This guide will help you get started.

## Getting Started

### Prerequisites

- **Rust**: 1.70 or higher
- **Git**: Latest version
- **SQLite**: Development headers (usually included with Rust)
- **Platform-specific tools**:
  - **Windows**: Visual Studio Build Tools
  - **macOS**: Xcode Command Line Tools
  - **Linux**: Build essentials

### Development Setup

1. **Clone the repository**:
   ```bash
   git clone https://github.com/lacodda/kasl.git
   cd kasl
   ```

2. **Install dependencies**:
   ```bash
   cargo build
   ```

3. **Run tests**:
   ```bash
   cargo test --lib --tests -- --test-threads=1
   ```

4. **Build documentation**:
   ```bash
   cargo doc --open
   ```

## Development Workflow

### Code Style

Follow the [Style Guide](./style-guide.html) for all code contributions:

- **Documentation**: Comprehensive module and function documentation
- **Formatting**: Use `rustfmt` for consistent formatting
- **Linting**: Address all `clippy` warnings
- **Tests**: Write tests for new functionality

### Branch Strategy

1. **Create feature branch**:
   ```bash
   git checkout -b feature/your-feature-name
   ```

2. **Make changes**:
   - Follow the style guide
   - Write tests
   - Update documentation

3. **Test thoroughly**:
   ```bash
   cargo test --lib --tests -- --test-threads=1
   cargo clippy
   cargo fmt --check
   ```

4. **Commit changes**:
   ```bash
   git add .
   git commit -m "feat: add new feature description"
   ```

5. **Push and create PR**:
   ```bash
   git push origin feature/your-feature-name
   ```

### Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

- **feat**: New feature
- **fix**: Bug fix
- **docs**: Documentation changes
- **style**: Code style changes
- **refactor**: Code refactoring
- **test**: Test changes
- **chore**: Maintenance tasks

Examples:
```
feat: add GitLab commit import functionality
fix: resolve database connection timeout issue
docs: update API integration guide
test: add comprehensive task management tests
```

## Project Structure

### Core Modules

- **`src/main.rs`**: Application entry point
- **`src/lib.rs`**: Library root and module organization
- **`src/commands/`**: CLI command implementations
- **`src/db/`**: Database layer and migrations
- **`src/libs/`**: Core library functionality
- **`src/api/`**: External API integrations

### Key Components

- **Activity Monitor**: `src/libs/monitor.rs`
- **Task Management**: `src/db/tasks.rs`
- **Configuration**: `src/libs/config.rs`
- **Database**: `src/db/db.rs`
- **API Clients**: `src/api/`

### Testing

- **Unit Tests**: In same file as implementation
- **Integration Tests**: `tests/` directory
- **Test Contexts**: Use `test-context` crate for isolation

## Development Guidelines

### Code Quality

1. **Documentation**: Document all public APIs
2. **Error Handling**: Use `anyhow` for error propagation
3. **Logging**: Use `tracing` for structured logging
4. **Testing**: Aim for high test coverage
5. **Performance**: Profile and optimize critical paths

### Database Changes

1. **Migrations**: Always create migrations for schema changes
2. **Backward Compatibility**: Maintain compatibility when possible
3. **Testing**: Test migrations thoroughly
4. **Documentation**: Update database documentation

### API Integrations

1. **Session Management**: Implement the `Session` trait
2. **Error Handling**: Handle network and authentication errors
3. **Rate Limiting**: Respect API rate limits
4. **Testing**: Mock external APIs in tests

### Configuration

1. **Backward Compatibility**: Don't break existing configurations
2. **Validation**: Validate configuration on load
3. **Documentation**: Update configuration documentation
4. **Defaults**: Provide sensible defaults

## Testing

### Running Tests

```bash
# All tests
cargo test --lib --tests -- --test-threads=1

# Specific test file
cargo test --test config

# Specific test
cargo test test_save_and_read_config

# With output
cargo test -- --nocapture
```

### Test Guidelines

1. **Isolation**: Each test should be independent
2. **Cleanup**: Clean up after tests
3. **Mocking**: Mock external dependencies
4. **Coverage**: Test error conditions and edge cases

### Test Contexts

Use test contexts for setup and teardown:

```rust
struct TestContext {
    temp_dir: TempDir,
}

impl TestContext {
    fn setup() -> Self {
        // Setup test environment
    }
    
    fn teardown(self) {
        // Cleanup
    }
}
```

## Debugging

### Debug Mode

Enable debug logging:
```bash
RUST_LOG=kasl=debug cargo run -- watch --foreground
```

### Database Debugging

```bash
# Direct database access
sqlite3 ~/.local/share/lacodda/kasl/kasl.db

# Check migrations
kasl migrations status
```

### Performance Profiling

```bash
# Build with profiling
cargo build --release

# Profile with perf (Linux)
perf record --call-graph=dwarf ./target/release/kasl watch
perf report
```

## Building

### Release Build

```bash
cargo build --release
```

### Cross-Platform Build

```bash
# Windows
cargo build --release --target x86_64-pc-windows-msvc

# macOS
cargo build --release --target x86_64-apple-darwin

# Linux
cargo build --release --target x86_64-unknown-linux-gnu
```

### Docker Build

```bash
docker build -t kasl .
docker run --rm kasl --version
```

## Documentation

### Code Documentation

- **Modules**: Document with `//!` comments
- **Functions**: Document with `///` comments
- **Structs**: Document fields and usage
- **Examples**: Include usage examples

### User Documentation

- **Commands**: Document all CLI commands
- **Configuration**: Document all options
- **Examples**: Provide practical examples
- **Troubleshooting**: Include common issues

### Building Documentation

```bash
# Build docs
cargo doc

# Build book
cd docs
mdbook build
mdbook serve
```

## Release Process

### Version Management

1. **Update version** in `Cargo.toml`
2. **Update changelog** in `CHANGELOG.md`
3. **Create release tag**:
   ```bash
   git tag v0.8.0
   git push origin v0.8.0
   ```

### Release Checklist

- [ ] All tests pass
- [ ] Documentation updated
- [ ] Changelog updated
- [ ] Version bumped
- [ ] Release tag created
- [ ] GitHub release created
- [ ] Binaries uploaded

## Getting Help

### Resources

- **Issues**: [GitHub Issues](https://github.com/lacodda/kasl/issues)
- **Discussions**: [GitHub Discussions](https://github.com/lacodda/kasl/discussions)
- **Documentation**: [kasl.lacodda.com](https://kasl.lacodda.com)

### Communication

- **Bug Reports**: Use GitHub Issues
- **Feature Requests**: Use GitHub Discussions
- **Questions**: Use GitHub Discussions
- **Security**: Email lahtachev@gmail.com

## Code of Conduct

### Our Standards

- Be respectful and inclusive
- Focus on constructive feedback
- Help others learn and grow
- Respect different perspectives

### Enforcement

- Report violations to maintainers
- Maintainers will address issues promptly
- Consequences may include warnings or bans

## License

By contributing to kasl, you agree that your contributions will be licensed under the MIT License.
