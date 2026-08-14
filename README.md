# erdify-rs

CLI tool to generate Mermaid ER (Entity-Relationship) diagrams from PostgreSQL databases.

## Install

Download a prebuilt binary from the [releases page](https://github.com/michaelcoll/erdify-rs/releases) (Windows x86_64, macOS arm64, Linux x86_64), or build from source:

```bash
mise install
mise run setup
mise run build
```

## Usage

```bash
erdify --url postgresql://user:pass@host:5432/dbname
```

If `--url` is omitted, erdify falls back to the `DATABASE_URL` environment variable. Output goes to stdout unless `--output` is set.

### Options

| Flag              | Description                                                               |
| ----------------- | ------------------------------------------------------------------------- |
| `-u, --url`       | PostgreSQL connection URL (or set `DATABASE_URL`)                         |
| `--schema`        | Only include these schemas (comma-separated)                              |
| `--table`         | Only include these tables (comma-separated)                               |
| `--ignore-tables` | Exclude these tables (comma-separated), mutually exclusive with `--table` |
| `--minimal`       | Columns only, no PK/FK                                                    |
| `--full`          | Columns + PK/FK/NOT NULL + relations + constraints + indexes              |
| `-o, --output`    | Write to file instead of stdout                                           |
| `--title`         | Diagram title (default: database name)                                    |

### Examples

```bash
# Only the public schema, full detail
erdify --url postgresql://user:pass@host/db --schema public --full

# Everything except audit tables
erdify --url postgresql://user:pass@host/db --ignore-tables logs,audit_trail

# Write to a file
erdify --url postgresql://user:pass@host/db --output erdify.md
```
