# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Rust-based terminal audio visualizer using Ratatui for the TUI interface. It captures real-time audio input, performs FFT analysis, and displays frequency spectrum as animated bars in the terminal.

## Core Architecture

The project follows a modular structure:

- **`main.rs`**: Application entry point with the main `App` struct containing UI rendering, event handling, and application state management
- **`audio.rs`**: Audio capture and processing module using `cpal` for audio I/O, `rustfft` for FFT analysis, and `ringbuf` for sample buffering
- **`config.rs`**: Configuration management for visualizer settings (bar count, color schemes, refresh rate, sensitivity)

### Key Components

- **AudioProcessor**: Handles audio device selection, sample capture, FFT processing in async tasks, and provides processed frequency data
- **App**: Main application loop with terminal rendering, keyboard controls, and real-time visualization updates
- **Config**: Runtime configuration with interactive adjustment methods for all visualizer parameters

### Data Flow

1. Audio samples captured via `cpal` → Ring buffer
2. FFT processing in background tokio task → Frequency magnitude data
3. Main app loop consumes FFT data → Renders bar chart via Ratatui
4. User input adjusts Config → Updates visualization parameters

## Common Commands

### Build and Run
```bash
cargo build
cargo run
```

### Development
```bash
cargo check          # Quick syntax/type checking
cargo clippy          # Linting
cargo fmt             # Code formatting
```

### Testing
```bash
cargo test
```

## Key Dependencies

- **ratatui**: Terminal UI framework for rendering
- **cpal**: Cross-platform audio I/O
- **rustfft**: FFT implementation for frequency analysis  
- **ringbuf**: Lock-free ring buffer for audio samples
- **tokio**: Async runtime for audio processing tasks
- **crossterm**: Terminal event handling
- **color-eyre**: Enhanced error reporting

## Interactive Controls

The application supports real-time configuration changes via keyboard:
- Color scheme cycling, bar count adjustment, refresh rate tuning, audio source switching, sensitivity control
- All settings are managed through the `Config` struct with bounds checking and smooth transitions