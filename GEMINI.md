# Gemini Code Assistant Workspace

This file provides context to the Gemini code assistant, helping it understand the project structure, purpose, and key components.

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
- `anyhow`, `snafu`, `thiserror`: A combination of error-handling libraries to manage potential issues gracefully.
- `log` & `env_logger`: For logging application events.

**Cross-Compilation:**
The project is configured for cross-compilation, as indicated by `Cross.toml` and `Dockerfile.cross`. This allows building the application for different architectures, such as ARM (e.g., for Raspberry Pi), from a standard x86 development environment.

## Code Structure

The core logic is organized into modules:

- **`src/main.rs`**: The main entry point of the application. It handles:
    - Parsing command-line arguments to determine the operating mode.
    - Initializing the serial port connection.
    - Setting up the MQTT client.
    - Orchestrating the main application loop.

- **`src/map_protocol/`**: This module encapsulates the logic for communicating with the MAP inverter.
    - **`low_level.rs`**: Implements the fundamental communication protocol. It is responsible for:
        - Constructing and sending commands in the inverter's specific binary format.
        - Reading and validating responses, including checksum verification.
        - Handling the byte-level intricacies of the serial communication.
    - **`high_level.rs`**: Provides a more abstract and user-friendly API over the low-level protocol. It exposes functions to:
        - Read the inverter's configuration from its EEPROM.
        - Fetch and parse the real-time status of the inverter.
        - Convert the raw data into a structured `MapInfo` object.

## How to Use

The application has two main modes:

1.  **MQTT Bridge Mode:** Connects to the inverter and an MQTT broker, continuously reading data and publishing it to a specified topic.
2.  **Stdout Mode:** Reads the inverter's status once and prints it to the console, either in a human-readable format or as a JSON object.

The application also supports generating shell command-line completion scripts.
