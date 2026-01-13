mod audio;
mod hotkey;
mod input;
mod websocket;

use anyhow::{Context, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use audio::AudioCapture;
use hotkey::{HotkeyCommand, HotkeyHandler};
use input::TextInputHandler;
use websocket::{AsrClient, AsrEvent};

#[derive(Debug, Clone, Copy, PartialEq)]
enum AppState {
    Idle,
    Recording,
}

struct App {
    state: AppState,
    audio_capture: AudioCapture,
    text_input: TextInputHandler,
    api_key: String,
    current_text: String,
    audio_tx: Option<mpsc::Sender<Vec<u8>>>,
}

impl App {
    fn new(api_key: String) -> Self {
        Self {
            state: AppState::Idle,
            audio_capture: AudioCapture::new(),
            text_input: TextInputHandler::new(),
            api_key,
            current_text: String::new(),
            audio_tx: None,
        }
    }

    async fn toggle_recording(&mut self) -> Result<()> {
        match self.state {
            AppState::Idle => self.start_recording().await,
            AppState::Recording => self.stop_recording().await,
        }
    }

    async fn start_recording(&mut self) -> Result<()> {
        info!("Starting recording...");

        // Create channels
        let (audio_tx, audio_rx) = mpsc::channel::<Vec<u8>>(100);
        let (event_tx, mut event_rx) = mpsc::channel::<AsrEvent>(100);

        // Store audio sender for later use
        self.audio_tx = Some(audio_tx.clone());

        // Start audio capture
        self.audio_capture.start(audio_tx)?;

        // Start ASR client
        let api_key = self.api_key.clone();
        let text_input = self.text_input.clone();

        tokio::spawn(async move {
            let mut client = AsrClient::new(api_key);
            if let Err(e) = client.start_recognition(audio_rx, event_tx).await {
                error!("ASR client error: {}", e);
            }
        });

        // Handle ASR events
        let current_text = Arc::new(tokio::sync::Mutex::new(String::new()));
        let current_text_clone = current_text.clone();

        tokio::spawn(async move {
            let mut previous_text = String::new();

            while let Some(event) = event_rx.recv().await {
                match event {
                    AsrEvent::TaskStarted => {
                        info!("ASR task started");
                    }
                    AsrEvent::ResultGenerated { text, is_final } => {
                        let mut current = current_text_clone.lock().await;

                        if is_final {
                            // Final result - append to current text
                            if !current.is_empty() && !text.is_empty() {
                                current.push(' ');
                            }
                            current.push_str(&text);

                            // Type the final text
                            if let Err(e) = text_input.type_text(&text) {
                                error!("Failed to type text: {}", e);
                            }

                            previous_text = current.clone();
                            info!("Final: {}", text);
                        } else {
                            // Partial result - update display
                            // For partial results, we show them but don't type yet
                            info!("Partial: {}", text);
                        }
                    }
                    AsrEvent::TaskFinished => {
                        info!("ASR task finished");
                        break;
                    }
                    AsrEvent::TaskFailed { error } => {
                        error!("ASR task failed: {}", error);
                        break;
                    }
                }
            }
        });

        self.state = AppState::Recording;
        info!("Recording started. Press Super+I to stop.");

        Ok(())
    }

    async fn stop_recording(&mut self) -> Result<()> {
        info!("Stopping recording...");

        // Stop audio capture (this will close the audio sender)
        self.audio_capture.stop();
        self.audio_tx = None;

        // Reset state
        self.current_text.clear();
        self.state = AppState::Idle;

        info!("Recording stopped.");
        Ok(())
    }
}

impl Clone for TextInputHandler {
    fn clone(&self) -> Self {
        TextInputHandler::new()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    // Load .env file if present
    let _ = dotenvy::dotenv();

    // Get API key
    let api_key = std::env::var("DASHSCOPE_API_KEY")
        .context("DASHSCOPE_API_KEY environment variable not set")?;

    info!("Audio2Text - Real-time speech recognition");
    info!("Press Super+I to start/stop recording");
    info!("Press Ctrl+C to exit");

    // Check for required tools
    check_dependencies();

    // Create app
    let app = Arc::new(tokio::sync::Mutex::new(App::new(api_key)));

    // Create command channel
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<HotkeyCommand>(10);

    // Setup hotkey handler
    let hotkey_handler = HotkeyHandler::new()?;

    // Spawn hotkey listener
    let hotkey_handle = tokio::spawn(async move {
        if let Err(e) = hotkey_handler.run(cmd_tx).await {
            error!("Hotkey handler error: {}", e);
        }
    });

    // Handle shutdown signal
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();

    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("\nShutting down...");
        running_clone.store(false, Ordering::SeqCst);
    });

    // Main event loop
    while running.load(Ordering::SeqCst) {
        tokio::select! {
            Some(cmd) = cmd_rx.recv() => {
                match cmd {
                    HotkeyCommand::ToggleRecording => {
                        let mut app = app.lock().await;
                        if let Err(e) = app.toggle_recording().await {
                            error!("Failed to toggle recording: {}", e);
                        }
                    }
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                // Periodic check
            }
        }
    }

    // Cleanup
    let mut app = app.lock().await;
    if app.state == AppState::Recording {
        app.stop_recording().await?;
    }

    hotkey_handle.abort();

    info!("Goodbye!");
    Ok(())
}

fn check_dependencies() {
    let tools = [
        ("wtype", "For typing text in Wayland"),
        ("ydotool", "Alternative for typing text"),
        ("wl-copy", "Fallback: copy to clipboard"),
    ];

    let mut found_input_tool = false;

    for (tool, description) in tools {
        if std::process::Command::new("which")
            .arg(tool)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            info!("✓ {} found - {}", tool, description);
            if tool == "wtype" || tool == "ydotool" {
                found_input_tool = true;
            }
        } else {
            warn!("✗ {} not found - {}", tool, description);
        }
    }

    if !found_input_tool {
        warn!("No text input tool found! Install wtype or ydotool for best experience.");
        warn!("Install with: sudo pacman -S wtype  (Arch Linux)");
    }
}
