use anyhow::Result;
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

pub enum HotkeyCommand {
    ToggleRecording,
}

pub struct HotkeyHandler {
    manager: GlobalHotKeyManager,
    hotkey_id: u32,
    is_running: Arc<AtomicBool>,
}

impl HotkeyHandler {
    pub fn new() -> Result<Self> {
        let manager = GlobalHotKeyManager::new()?;

        // Register Super (Win) + I
        let hotkey = HotKey::new(Some(Modifiers::SUPER), Code::KeyI);
        let hotkey_id = hotkey.id();

        manager.register(hotkey)?;
        info!("Registered hotkey: Super+I (id: {})", hotkey_id);

        Ok(Self {
            manager,
            hotkey_id,
            is_running: Arc::new(AtomicBool::new(false)),
        })
    }

    pub async fn run(&self, cmd_tx: mpsc::Sender<HotkeyCommand>) -> Result<()> {
        self.is_running.store(true, Ordering::SeqCst);
        let is_running = self.is_running.clone();
        let hotkey_id = self.hotkey_id;

        let receiver = GlobalHotKeyEvent::receiver();

        info!("Hotkey handler started, press Super+I to toggle recording");

        loop {
            if !is_running.load(Ordering::SeqCst) {
                break;
            }

            // Use a timeout to check is_running periodically
            match receiver.try_recv() {
                Ok(event) => {
                    debug!("Received hotkey event: {:?}", event);
                    if event.id == hotkey_id {
                        info!("Toggle recording hotkey pressed");
                        if let Err(e) = cmd_tx.send(HotkeyCommand::ToggleRecording).await {
                            error!("Failed to send hotkey command: {}", e);
                        }
                    }
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {
                    // No event, sleep briefly
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    error!("Hotkey receiver disconnected");
                    break;
                }
            }
        }

        Ok(())
    }

    pub fn stop(&self) {
        self.is_running.store(false, Ordering::SeqCst);
    }
}

impl Drop for HotkeyHandler {
    fn drop(&mut self) {
        // Hotkeys are automatically unregistered when manager is dropped
        info!("Hotkey handler stopped");
    }
}
