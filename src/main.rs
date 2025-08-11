use notify::{
    event::{
        AccessKind, CreateKind, DataChange, Event, EventKind, ModifyKind, RemoveKind, RenameMode,
    },
    RecommendedWatcher, RecursiveMode, Result, Watcher,
};
use std::path::Path;
use std::sync::mpsc::channel;
use std::time::Duration;

fn main() -> Result<()> {
    println!("--- Windows File Watcher ---");
    println!("Monitoring changes in the './data_to_watch' directory.");
    println!("Press Ctrl+C to exit.");

    // --- 1. Setup the Path to Watch ---
    // The user should create this directory before running the program.
    let path_to_watch = Path::new("./data_to_watch");
    if !path_to_watch.exists() {
        println!("Error: The directory '{:?}' does not exist.", path_to_watch);
        println!("Please create it and try again.");
        // Use a more specific error type in a real app
        return Err(notify::Error::path_not_found());
    }

    // --- 2. Create a Channel for Communication ---
    // The watcher runs in a separate thread. We use a channel to receive events
    // from it in our main thread, which is a very common and robust pattern in Rust.
    let (tx, rx) = channel();

    // --- 3. Create the Watcher ---
    // `RecommendedWatcher` selects the best implementation for the current platform.
    // On Windows, it uses the efficient `ReadDirectoryChangesW` API.
    // We create it with a timeout of 1 second to debounce events.
    let config = notify::Config::default()
        .with_poll_interval(Duration::from_secs(1))
        .with_compare_contents(true); // Useful on some platforms to reduce noise

    // The `move` keyword before the closure is crucial. It tells the closure
    // to take ownership of `tx`, which it needs to send events to the main thread.
    let mut watcher: RecommendedWatcher = RecommendedWatcher::new(
        move |res: Result<Event>| {
            // We just forward the event/error to the main thread.
            // `unwrap()` is fine here because the receiver `rx` will not be dropped
            // before the sender `tx` is.
            tx.send(res).unwrap();
        },
        config,
    )?;

    // --- 4. Start Watching ---
    // We want to watch all files and folders *inside* the specified path,
    // so we use `RecursiveMode::Recursive`.
    watcher.watch(path_to_watch, RecursiveMode::Recursive)?;

    // --- 5. The "Hook" - Process Events ---
    // This is the main event loop. `rx.recv()` will block until an event
    // is received, so this loop is very efficient (no busy-waiting).
    println!("\n[INFO] Watcher started. Waiting for file system events...");
    for res in rx {
        match res {
            Ok(event) => {
                // This is our "hook"! We can do anything we want with the event.
                // For this example, we'll just print a descriptive message.
                handle_event(&event);
            }
            Err(e) => println!("[ERROR] Watch error: {:?}", e),
        }
    }

    Ok(())
}

/// A helper function to neatly print details about the file system event.
/// This is the core logic of our "hook".
fn handle_event(event: &Event) {
    // The `event.kind` tells us what happened.
    // The `event.paths` tells us where it happened.
    let path = event
        .paths
        .first()
        .map_or("N/A", |p| p.to_str().unwrap_or("Invalid UTF-8"));

    match &event.kind {
        // --- Creation Events ---
        EventKind::Create(CreateKind::File) => println!("[CREATE] File created: {}", path),
        EventKind::Create(CreateKind::Folder) => println!("[CREATE] Folder created: {}", path),
        EventKind::Create(CreateKind::Any) => println!("[CREATE] File/Folder created: {}", path),

        // --- Modification Events ---
        EventKind::Modify(ModifyKind::Data(DataChange::Content)) => {
            println!("[MODIFY] File content changed: {}", path)
        }
        EventKind::Modify(ModifyKind::Metadata(_)) => {
            println!("[MODIFY] Metadata changed: {}", path)
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::To)) => {
            // This event often comes in a pair with a `RenameMode::From` event.
            // The `paths` vec will contain the new path.
            println!("[RENAME] Renamed/Moved to: {}", path);
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
            println!("[RENAME] Renamed/Moved from: {}", path);
        }
        EventKind::Modify(ModifyKind::Any) => println!("[MODIFY] File/Folder modified: {}", path),

        // --- Removal Events ---
        EventKind::Remove(RemoveKind::File) => println!("[REMOVE] File removed: {}", path),
        EventKind::Remove(RemoveKind::Folder) => println!("[REMOVE] Folder removed: {}", path),
        EventKind::Remove(RemoveKind::Any) => println!("[REMOVE] File/Folder removed: {}", path),

        // --- Access Events (often noisy, but can be useful) ---
        EventKind::Access(AccessKind::Read) => println!("[ACCESS] File read: {}", path),
        EventKind::Access(AccessKind::Open(_)) => println!("[ACCESS] File opened: {}", path),
        EventKind::Access(AccessKind::Close(_)) => println!("[ACCESS] File closed: {}", path),

        // --- Other ---
        _ => println!("[OTHER] Another event type occurred: {:?}", event.kind),
    }
}
