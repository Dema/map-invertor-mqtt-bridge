# Agent Instructions

Context for coding agents working in this repository: what the project does, how it is structured, and how to run it.

## Project Overview

**Project Name:** `map-invertor-mqtt-bridge`

**Purpose:** This project is a Rust-based application that reads data from a "MAP" (МАП) inverter over a serial connection and publishes it to an MQTT broker. It serves as a bridge, translating the inverter's proprietary protocol into a more accessible format (JSON over MQTT). The application can also output the inverter's status directly to the standard output.

**Original Implementation:** The current Rust application is a rewrite of an earlier C version, `mapd.c`, which functioned as a daemon for reading inverter data.

## Technical Details

**Primary Language:** Rust

**Key Libraries/Crates:**
- `clap`: Used for parsing command-line arguments, allowing for different modes of operation (MQTT bridge, stdout dump).
- `paho-mqtt`: Handles all communication with the MQTT broker.
- `serialport`: Manages the serial port communication with the MAP inverter.
- `serde` & `serde_json`: Used for serializing the inverter data into JSON format for MQTT messages and stdout output.
- `anyhow`, `thiserror`: Error handling — `anyhow` at application boundaries, `thiserror` for typed errors.
- `log` & `env_logger`: For logging application events.

**Cross-Compilation:**
The project is configured for cross-compilation, as indicated by `Cross.toml` and `Dockerfile.cross`. This allows building the application for different architectures, such as ARM (e.g., for Raspberry Pi), from a standard x86 development environment.

## Code Structure

The core logic is organized into modules:

- **`src/main.rs`**: Composition only. It parses the command line, dumps one status to stdout, and runs the MQTT bridge with its single retry policy: an attempt keeps the device session and the broker connection alive independently, and only the end that failed is rebuilt after `--reconnect-every`.

- **`src/cli.rs`**: The argument surface — working mode, serial and MQTT parameters, shell completions.

- **`src/session.rs`**: The device side, with no MQTT in sight.
    - **`InverterDevice`**: The seam — `read_eeprom` and `read_status`. `SerialDevice` is the production adapter; a scripted device stands in during tests.
    - **`PollingSession`**: Owns the EEPROM read once per session, then polls statuses from the same device.
    - **`Watchdog`**: Trips when the status stays the same for more than `MAX_CONSECUTIVE_SAME_READS` reads, turning a hung inverter into a session error.

- **`src/bridge.rs`**: The publish-on-change rule and the `StatusSink` seam (`publish(&MapInfo)`, MQTT in production, a recorder in tests). `BridgeError::failed_end` says which side of the bridge has to be rebuilt.

- **`src/mqtt_sink.rs`**: `MqttSink`, the production `StatusSink`: the status as retained JSON on one topic, at QoS 1.

- **`src/map_protocol/`**: This module encapsulates the logic for communicating with the MAP inverter.
    - **`low_level.rs`**: Implements the fundamental communication protocol. It is responsible for:
        - Constructing and sending commands in the inverter's specific binary format.
        - Reading and validating responses, including checksum verification.
        - Handling the byte-level intricacies of the serial communication.
    - **`high_level.rs`**: Provides a more abstract and user-friendly API over the low-level protocol. It exposes functions to:
        - Read the inverter's configuration from its EEPROM.
        - Fetch and parse the real-time status of the inverter.
        - Convert the raw data into a structured `MapInfo` object.
        - Check that an EEPROM read came from a MAP at all (`ensure_map_present`, the one home of the `eeprom[0] == 3` magic byte).

## How to Use

The application has two main modes:

1.  **MQTT Bridge Mode:** Connects to the inverter and an MQTT broker, continuously reading data and publishing it to a specified topic.
2.  **Stdout Mode:** Reads the inverter's status once and prints it to the console, either in a human-readable format or as a JSON object.

The application also supports generating shell command-line completion scripts.

## Conventions

**Checks:** run `git config core.hooksPath .githooks` once per clone. The pre-commit hook in `.githooks/pre-commit` runs `cargo fmt --check` and `cargo test`.

**Commits:** `<type>: <summary>` with one of `fix:`, `refactor:`, `feat:`, `docs:`, `style:`, `chore:`; the body says why, and the last line names the ticket (`Refs #12`).

## Agent skills

### Issue tracker

Issues and specs live as GitHub issues in `Dema/map-invertor-mqtt-bridge` (via the `gh` CLI). See `docs/agents/issue-tracker.md`.

### Triage labels

The default five canonical triage roles: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.
