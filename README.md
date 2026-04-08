# pakr

Pack any directory into a zip with environment-based naming and old archive cleanup.

Works with any project — frontend builds, backend artifacts, docs, static assets.

## Installation

### From Source

```bash
git clone https://github.com/cecil-su/pakr.git
cd pakr
cargo install --path .
```

## Usage

### Quick start

```bash
# Initialize config file
pakr init

# Pack the dist directory
pakr pack --env prod

# Pack without environment tag
pakr pack
```

### Pack

```bash
# Basic pack
pakr pack --env prod
# Output: myproject-prod-20260408095217.zip

# Custom source directory
pakr pack --env prod --source build

# Custom prefix and output directory
pakr pack --env test --prefix my-project --output ./archives/

# Preview without creating the zip
pakr --dry-run pack --env prod
```

### Clean

```bash
# Clean old prod archives, keep latest 1
pakr clean --mode current --env prod --keep 1

# Clean all archives (requires confirmation)
pakr clean --mode all

# Force clean in CI (skip confirmation)
pakr clean --mode all --force

# Preview what would be deleted
pakr --dry-run clean --mode current --env prod
```

### Init

```bash
# Generate pakr.toml with sensible defaults
pakr init
```

## Configuration

Create a `pakr.toml` in your project root (or run `pakr init`):

```toml
# Project name prefix for zip filenames
prefix = "my-project"

# Separator between name parts (default: -)
# separator = "-"

# Timestamp format, chrono syntax (default: %Y%m%d%H%M%S)
# date_format = "%m%d%H%M%S"

# Source directory to compress (default: dist)
# source = "dist"

# Output directory for zip files (default: .)
# output = "."

# Cleanup settings
[cleanup]
enabled = false
mode = "current"    # "all" or "current"
keep = 1            # keep latest N archives (current mode only)
```

CLI arguments override config file values.

## Options

### Global

| Option | Short | Description |
|--------|-------|-------------|
| `--config <PATH>` | | Config file path (default: `pakr.toml`) |
| `--dry-run` | `-n` | Preview operations without executing |

### Pack

| Option | Short | Description |
|--------|-------|-------------|
| `--env <ENV>` | `-e` | Environment name (e.g. `prod`, `test`) |
| `--prefix <NAME>` | `-p` | Project prefix (default: directory name) |
| `--source <DIR>` | `-s` | Source directory (default: `dist`) |
| `--output <DIR>` | `-o` | Output directory (default: `.`) |
| `--separator <CHAR>` | | Separator character (default: `-`) |
| `--date-format <FMT>` | | Timestamp format (default: `%Y%m%d%H%M%S`) |
| `--no-clean` | | Skip automatic cleanup |

### Clean

| Option | Short | Description |
|--------|-------|-------------|
| `--env <ENV>` | `-e` | Environment to clean (required for `current` mode) |
| `--mode <MODE>` | | `current` (default) or `all` |
| `--keep <N>` | | Keep latest N archives (default: `1`) |
| `--force` | | Skip confirmation prompt (for CI) |

## Filename Format

```
{prefix}{sep}{env}{sep}{timestamp}.zip
```

| Part | Example | Source |
|------|---------|--------|
| prefix | `my-project` | `--prefix` / config / directory name |
| env | `prod` | `--env` (omitted if not set) |
| timestamp | `20260408095217` | `--date-format` |

Examples:
- `my-project-prod-20260408095217.zip`
- `my-project-test-20260408143020.zip`
- `my-project-20260408095217.zip` (no env)

## License

MIT
