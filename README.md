# TRON Vanity Address Generator

A TRON vanity address generator written in Rust. It runs as a **CLI tool**, an **embedded web UI**, or a **desktop application**. All key generation happens locally and offline — nothing is uploaded, and no network access is required.

## Features

- **Multi-threaded brute force** powered by `rayon`, with live attempt counts, keys/s and per-hit progress
- **Composable conditions**: repeated character runs, fixed characters at a given position, suffix matching and regular expressions, combined with `AND` / `OR`
- **XML-driven configuration**: `config.xml` describes the job; CLI flags override it on the fly
- **Single-file web UI**: the HTML is embedded into the binary with `include_str!`, so the web mode ships with zero external assets
- **Desktop build**: native window plus the system webview (WKWebView on macOS, WebView2 on Windows) — just double-click to run
- **Export** to CSV / TXT / JSON
- **Optional Telegram notifications**, one message per matched address, easy to turn off

## Address derivation

```
32-byte random private key
  -> secp256k1 uncompressed public key (65 bytes, prefixed with 0x04)
  -> Keccak-256
  -> last 20 bytes of the digest
  -> prefixed with 0x41 (the TRON network byte)
  -> Base58Check encoding
  -> 34-character address starting with "T"
```

## Quick start

```bash
# Build
cargo build --release

# Run the CLI (reads ./config.xml by default)
./target/release/tron-vanity

# Point at another config and override parameters
./target/release/tron-vanity --config my.xml --count 3 --threads 8
```

### Desktop app

```bash
cargo build --release --bin tron-vanity-desktop
./target/release/tron-vanity-desktop
```

The desktop shell starts the web server on a background thread (on a random local port) and opens it in a native webview. A `config.xml` sitting next to the executable is picked up automatically.

### Web UI

```bash
./target/release/tron-vanity --web --port 8686
```

The default browser opens at `http://127.0.0.1:8686`. From the page you can adjust the suffix, target count and thread count, watch live progress and throughput, browse the result table and download the export file directly.

## CLI reference

| Flag | Short | Default | Description |
| --- | --- | --- | --- |
| `--config <FILE>` | `-c` | `config.xml` | Path to the XML configuration file |
| `--web` | | `false` | Start the embedded web UI instead of the terminal workflow |
| `--host <HOST>` | | `127.0.0.1` | Interface the web UI binds to |
| `--port <PORT>` | | `8686` | Port the web UI binds to |
| `--count <N>` | `-n` | config value | Overrides `TaskConfig/TargetCount` |
| `--threads <N>` | `-t` | config value | Overrides `TaskConfig/ThreadCount`; `0` auto-detects CPU cores |
| `--format <FMT>` | `-f` | config value | Overrides the export format: `csv`, `txt` or `json` |
| `--output <DIR>` | `-o` | `.` | Directory the export file is written to |
| `--no-progress` | | `false` | Disables the live terminal progress display |
| `--telegram-token <T>` | | built-in | Overrides the Telegram bot token |
| `--telegram-chat <ID>` | | built-in | Overrides the Telegram chat id that receives notifications |
| `--no-telegram` | | `false` | Turns Telegram notifications off entirely |

## Configuration (`config.xml`)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<AddressGeneratorConfig>

    <!-- Chain info; TRON only accepts the values below -->
    <ChainConfig>
        <ChainType>TRON</ChainType>
        <AddressPrefix>T</AddressPrefix>
        <AddressLength>34</AddressLength>
        <PrivateKeyFormat>Hex</PrivateKeyFormat>
    </ChainConfig>

    <!-- Job settings -->
    <TaskConfig>
        <TargetCount>1</TargetCount>     <!-- how many matches to generate -->
        <ThreadCount>0</ThreadCount>     <!-- 0 = auto-detect CPU cores -->
        <ExportFormat>CSV</ExportFormat> <!-- CSV / TXT / JSON -->
    </TaskConfig>

    <!-- Condition set -->
    <CompositeConditions>
        <LogicOperator>AND</LogicOperator>  <!-- AND = all must hold, OR = any -->

        <!-- The address must end with three consecutive 8s -->
        <Condition>
            <Type>Repeating</Type>
            <Param>
                <Char>8</Char>
                <MinCount>3</MinCount>
                <Position>Last</Position>   <!-- Last: trailing | Anywhere: anywhere -->
            </Param>
        </Condition>

        <!-- The 5th character from the end must be an "A" -->
        <Condition>
            <Type>FixedChar</Type>
            <Param>
                <Char>A</Char>
                <IndexFromEnd>5</IndexFromEnd>  <!-- 1 = last character -->
            </Param>
        </Condition>
    </CompositeConditions>

</AddressGeneratorConfig>
```

### Condition types

| `Type` | `Param` fields | Meaning |
| --- | --- | --- |
| `Repeating` | `Char`, `MinCount`, `Position` | Requires a run of `MinCount` identical `Char`s. `Position` is `Last` (must be trailing, the default) or `Anywhere` |
| `FixedChar` (alias `Fixed`) | `Char`, `IndexFromEnd` | The character at index `IndexFromEnd` counted from the end must be `Char` (1 = last character) |
| `Suffix` (aliases `EndsWith`, `Ends_With`) | `Value` | The address must end with `Value` |
| `Regex` | `Pattern` | The address must match the pattern (`regex` crate semantics, unanchored) |

With no `<Condition>` declared, every generated address counts as a match and the search returns immediately.

### Validation rules

- `ChainType` must be `TRON` and `PrivateKeyFormat` must be `Hex`
- `AddressPrefix` must be `T` and `AddressLength` must be `34`
- `TargetCount` must be greater than zero

## Export

Files are named `tron_vanity_20261119_064200.csv` and written to the `--output` directory (the current directory by default).

```csv
index,address,private_key,found_at_attempts
1,TYAcs9QRYgmQ4Z8oYVAtbHfcv2WAeAG888,<64-character hex private key>,18607333
```

`private_key` is the 32-byte key in hex without a `0x` prefix; it is replaced with a placeholder above.

## Web API

Endpoints used by the web UI (localhost only, no authentication):

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/` | The embedded HTML page |
| `GET` | `/api/defaults` | Default suffix / target / threads read from `config.xml` |
| `GET` | `/api/state` | Current run state, progress and found addresses |
| `POST` | `/api/start` | Start a search with the submitted parameters |
| `POST` | `/api/stop` | Stop the running search |
| `GET` | `/api/download` | Download the export file for the current results |

## Packaging

`.github/workflows/build.yml` defines the `Build desktop app` workflow (Windows and macOS, desktop build only). Run it manually from the Actions tab or by pushing a `v*` tag:

| Platform | Artifacts |
| --- | --- |
| macOS Apple Silicon | `tron-vanity-macos.dmg`, `tron-vanity-macos-app.zip` (.app) |
| macOS Intel | same as above |
| Windows x64 | `tron-vanity-windows-x64.zip` (portable), `tron-vanity-windows-x64.msi` (installer) |

The Windows `.msi` is produced from [wix/installer.wxs](wix/installer.wxs) and installs Start Menu and desktop shortcuts.

## Project layout

```
src/
  lib.rs                      # Shared modules exposed to the CLI and the desktop shell
  main.rs                     # CLI entry point (bin: tron-vanity)
  bin/tron-vanity-desktop.rs  # Desktop entry point (bin: tron-vanity-desktop)
  config.rs                   # XML configuration parsing and validation
  matcher.rs                  # Condition compilation and matching
  generator.rs                # Parallel search, address derivation, observer trait
  export.rs                   # CSV / TXT / JSON export
  notify.rs                   # Telegram notification decorator
  web.rs                      # axum server and JSON API
  web_ui.html                 # Embedded single-page front-end
  error.rs                    # Unified error type
wix/installer.wxs             # Windows MSI definition
assets/icon.{png,icns,ico}    # Application icon
config.xml                    # Default configuration
```

## Security notes

Generated private keys are written to the export file **in plain text**. Keep them safe:

- Do not export into a synced folder or a public repository
- `src/notify.rs` contains a built-in Telegram bot token and chat id. If you plan to distribute this project, move them to environment variables or a local config file
