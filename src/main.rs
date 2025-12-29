// src/main.rs

// Define our modules
mod debouncer;
mod event_handler;

use notify::event::EventKind; // Import EventKind for filtering
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::fs;
use std::path::Path;
use std::time::Duration;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Async File Watcher with 3-Second Debounce ---");

    // --- 1. Setup the Path to Watch ---
    let path_to_watch = Path::new(".");
    if !path_to_watch.exists() {
        println!("Creating directory: {:?}", path_to_watch);
        fs::create_dir_all(path_to_watch)?;
    }
    println!("Monitoring changes in: '{}'", path_to_watch.display());
    println!("Press Ctrl+C to exit.");

    // --- 2. Create an MPSC Channel for Tokio ---
    let (tx, rx) = mpsc::channel(100);

    // --- 3. Spawn the Debouncer Task ---
    let debounce_duration = Duration::from_secs(3);
    tokio::spawn(debouncer::debouncer(rx, debounce_duration));

    // --- 4. Create and Configure the File Watcher ---
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                if let EventKind::Access(_) = event.kind {
                    return;
                }

                if tx.try_send(event).is_err() {
                    println!(
                        "[Warning] Channel is full, event dropped. This might happen under heavy load."
                    );
                }
            }
        },
        Config::default(),
    )?;

    // Start watching the path recursively.
    watcher.watch(path_to_watch, RecursiveMode::Recursive)?;

    // --- 5. Keep the Main Task Alive ---
    tokio::signal::ctrl_c().await?;
    println!("\nShutdown signal received. Exiting.");

    Ok(())
}
