# CLI Downloader Module

This module handles downloading and managing the BAML CLI binary for the VSCode extension.

## Architecture

The code has been refactored into several focused modules:

### Core Modules

- **`types.ts`** - TypeScript type definitions
- **`constants.ts`** - Configuration constants (URLs, timeouts, etc.)
- **`platform.ts`** - Platform detection and architecture mapping
- **`paths.ts`** - Path management and file operations
- **`backoff.ts`** - Download retry logic with exponential backoff
- **`download.ts`** - HTTP download and checksum verification
- **`extraction.ts`** - Archive extraction (zip/tar.gz)
- **`bundled-cli.ts`** - Bundled CLI detection and management
- **`cache.ts`** - CLI caching functionality
- **`index.ts`** - Main entry point that orchestrates all modules

### Key Features

1. **Version Management**: Downloads specific CLI versions as needed
2. **Platform Support**: Handles Windows, macOS, and Linux (including musl/gnu detection)
3. **Security**: Verifies SHA256 checksums for all downloads
4. **Resilience**: Exponential backoff for failed downloads
5. **Caching**: Caches downloaded CLIs in `~/.baml`
6. **Bundled CLI**: Prefers bundled CLI when version matches

### Usage

```typescript
import { resolveCliPath } from './cli-downloader';

const cliPath = await resolveCliPath(
  extensionContext,
  requestedVersion,
  outputChannel
);
```

### Testing

Tests are located in `__tests__/` and can be run with:

```bash
npm test
```