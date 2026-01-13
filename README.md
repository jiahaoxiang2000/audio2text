# audio2text

Real-time audio-to-text transcription tool for Wayland using the DashScope API.

## Features

- Real-time speech recognition using DashScope WebSocket API
- Global hotkey (Super+I) to toggle recording
- Automatic text input into active application
- Supports microphone input with automatic audio format conversion
- Wayland-compatible text input simulation

## Requirements

### System Dependencies

Install the required tools for Wayland text input:

```bash
# Arch Linux / Manjaro
sudo pacman -S wtype

# Or alternative (ydotool)
sudo pacman -S ydotool

# Or fallback (clipboard only)
sudo pacman -S wl-clipboard
```

### Environment

Set your DashScope API key:

```bash
export DASHSCOPE_API_KEY="your_api_key_here"
```

Or create a `.env` file in the project directory:

```env
DASHSCOPE_API_KEY=your_api_key_here
```

## Building

```bash
cargo build --release
```

The binary will be available at `target/release/audio2text`.

## Usage

1. Start the application:
   ```bash
   cargo run --release
   ```

2. Press **Super+I** to start/stop recording

3. The transcribed text will be automatically typed into your active application

4. Press **Ctrl+C** to exit

## Project Structure

- `src/main.rs` - Main application logic and state management
- `src/websocket/mod.rs` - DashScope WebSocket client implementation
- `src/audio/mod.rs` - Audio capture using cpal
- `src/hotkey/mod.rs` - Global hotkey handling
- `src/input/mod.rs` - Text input simulation for Wayland

## Configuration

The application uses the following defaults:

- Sample rate: 16000 Hz
- Audio format: PCM 16-bit
- Recording chunk size: 100ms
- Hotkey: Super+I

## Troubleshooting

### "No input device available"
- Ensure your microphone is connected and recognized by the system
- Check with `pavucontrol` or `wpctl` (PipeWire)

### "No suitable text input method found"
- Install `wtype` (recommended) or `ydotool` for Wayland
- Alternatively, install `wl-clipboard` for clipboard-based input

### Audio quality issues
- Check your microphone settings in PulseAudio/PipeWire
- Ensure the correct input device is selected

## License

MIT
