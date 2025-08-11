// src/event_handler.rs

use notify::event::{AccessKind, CreateKind, Event, EventKind, ModifyKind, RemoveKind, RenameMode};
use std::process::Command; // Import the Command module

/// A helper function to neatly print details about a batch of file system events
/// and then execute the git commit action.
pub fn handle_events(events: &[Event]) {
    if events.is_empty() {
        return;
    }

    println!("\n=======================================================");
    println!("✅ DEBOUNCED ACTION! Processing {} events...", events.len());
    println!("=======================================================");

    // Log the individual events that triggered this action
    for event in events {
        handle_single_event(event);
    }
    println!("-------------------------------------------------------");

    // Execute the git commands
    println!("🚀 Executing git auto-commit...");
    run_git_commit();
    println!("-------------------------------------------------------\n");
}

/// Executes `git add .` and `git commit -m "Update"`.
fn run_git_commit() {
    // --- Step 1: git add . ---
    println!("-> Running: git add .");
    let add_output = Command::new("git").arg("add").arg(".").output();

    match add_output {
        Ok(output) => {
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!("[ERROR] `git add .` failed:\n{}", stderr);
                return; // Stop if 'git add' fails
            }
            println!("[SUCCESS] Staged changes.");
        }
        Err(e) => {
            eprintln!("[ERROR] Failed to execute `git add .`: {}", e);
            eprintln!("        Ensure 'git' is installed and in your system's PATH.");
            return;
        }
    }

    // --- Step 2: git commit -m "Update" ---
    println!("-> Running: git commit -m \"Update\"");
    let commit_output = Command::new("git")
        .arg("commit")
        .arg("-m")
        .arg("Update")
        .output();

    match commit_output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            if output.status.success() {
                println!("[SUCCESS] Committed changes:\n{}", stdout);
            } else {
                // `git commit` fails if there's nothing to commit. This is not a
                // critical error, so we check for that specific message.
                if stderr.contains("nothing to commit")
                    || stderr.contains("no changes added to commit")
                {
                    println!("[INFO] No new changes to commit.");
                } else {
                    // It was a different, real error.
                    eprintln!("[ERROR] `git commit` failed:\n{}", stderr);
                }
            }
        }
        Err(e) => {
            eprintln!("[ERROR] Failed to execute `git commit`: {}", e);
        }
    }
}

/// Processes and prints a single file system event for logging purposes.
fn handle_single_event(event: &Event) {
    let path = event
        .paths
        .first()
        .map_or("N/A", |p| p.to_str().unwrap_or("Invalid UTF-8"));

    match &event.kind {
        EventKind::Create(CreateKind::File) => println!("[CREATE] File created: {}", path),
        EventKind::Create(CreateKind::Folder) => println!("[CREATE] Folder created: {}", path),
        EventKind::Modify(ModifyKind::Data(_)) => {
            println!("[MODIFY] File content changed: {}", path)
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::To)) => {
            println!("[RENAME] Renamed/Moved to: {}", path)
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
            println!("[RENAME] Renamed/Moved from: {}", path)
        }
        EventKind::Remove(RemoveKind::File) => println!("[REMOVE] File removed: {}", path),
        EventKind::Remove(RemoveKind::Folder) => println!("[REMOVE] Folder removed: {}", path),
        EventKind::Access(AccessKind::Close(_)) => println!("[ACCESS] File closed: {}", path),
        _ => println!("[OTHER] Event: {:?} on {}", event.kind, path),
    }
}
