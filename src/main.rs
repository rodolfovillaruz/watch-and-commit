// src/main.rs

// Define our modules
mod debouncer;
mod event_handler;

use notify::event::EventKind; // Import EventKind for filtering
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use tokio::sync::mpsc;

/// Runs a git command and returns (success, stdout, stderr).
fn run_git(args: &[&str]) -> Result<(bool, String, String), Box<dyn std::error::Error>> {
    let output = Command::new("git").args(args).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    Ok((output.status.success(), stdout, stderr))
}

/// Runs a git command with specific environment overrides (or removals).
fn run_git_env(
    args: &[&str],
    env_remove: &[&str],
) -> Result<(bool, String, String), Box<dyn std::error::Error>> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    for var in env_remove {
        cmd.env_remove(var);
    }
    let output = cmd.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    Ok((output.status.success(), stdout, stderr))
}

fn ensure_repo_initialised() -> Result<(), Box<dyn std::error::Error>> {
    let work_tree = std::env::var("GIT_WORK_TREE").ok();
    let git_dir = std::env::var("GIT_DIR").ok();

    let (work_tree, git_dir) = match (work_tree, git_dir) {
        (Some(wt), Some(gd)) => (wt, gd),
        _ => return Ok(()),
    };

    let wt_path = Path::new(&work_tree);
    let gd_path = Path::new(&git_dir);

    // Create work tree if missing
    if !wt_path.exists() {
        println!(
            "📁 Work tree '{}' does not exist. Creating...",
            work_tree
        );
        std::fs::create_dir_all(wt_path)?;
        println!("   Created work tree directory: {}", work_tree);
    }

    // Initialise bare repo if GIT_DIR doesn't look like a valid repo
    if !gd_path.join("HEAD").exists() {
        println!("   Initialising bare repository at: {}", git_dir);
        std::fs::create_dir_all(gd_path)?;
        // `git init --bare` must not see GIT_WORK_TREE or GIT_DIR — they
        // conflict with --bare initialisation.
        let (ok, _, stderr) =
            run_git_env(&["init", "--bare", &git_dir], &["GIT_WORK_TREE", "GIT_DIR"])?;
        if !ok {
            return Err(format!("Failed to init bare repository: {}", stderr.trim()).into());
        }
    } else {
        println!(
            "   Bare repository already exists at: {}",
            git_dir
        );
    }

    Ok(())
}

/// Performs pre-flight checks to ensure the repository is in a clean and synced state.
/// 1. No tracked changes (staged or unstaged).
/// 2. No untracked files.
/// 3. After fetching, HEAD is in sync with the remote tracking branch.
fn preflight_checks() -> Result<(), Box<dyn std::error::Error>> {
    // --- Check 0: Auto-initialise if work tree is missing ---
    ensure_repo_initialised()?;

    // --- Check 1: Ensure we are in a git repository ---
    let (ok, _, stderr) = run_git(&["rev-parse", "--is-inside-work-tree"])?;
    if !ok {
        return Err(format!("Not inside a git repository: {}", stderr.trim()).into());
    }

    // --- Check 2: No tracked changes (staged or unstaged) ---
    // `git diff --quiet` exits with 1 if there are unstaged changes.
    let has_unstaged = Command::new("git")
        .args(["diff", "--quiet"])
        .status()?
        .code()
        .map_or(true, |c| c != 0);

    // `git diff --cached --quiet` exits with 1 if there are staged changes.
    let has_staged = Command::new("git")
        .args(["diff", "--cached", "--quiet"])
        .status()?
        .code()
        .map_or(true, |c| c != 0);

    if has_unstaged || has_staged {
        let mut msg = String::from("Repository has uncommitted tracked changes:\n");
        if has_unstaged {
            msg.push_str("  - Unstaged modifications detected.\n");
        }
        if has_staged {
            msg.push_str("  - Staged (indexed) changes detected.\n");
        }
        msg.push_str("Please commit or stash your changes before running the watcher.");
        return Err(msg.into());
    }

    // --- Check 3: No untracked files ---
    let (ok, stdout, _) = run_git(&["ls-files", "--others", "--exclude-standard"])?;
    if ok && !stdout.trim().is_empty() {
        let untracked_files: Vec<&str> = stdout.trim().lines().collect();
        let preview: Vec<&str> = untracked_files.iter().take(10).copied().collect();
        let mut msg = format!(
            "Repository has {} untracked file(s):\n",
            untracked_files.len()
        );
        for f in &preview {
            msg.push_str(&format!("  - {}\n", f));
        }
        if untracked_files.len() > 10 {
            msg.push_str(&format!("  ... and {} more.\n", untracked_files.len() - 10));
        }
        msg.push_str("Please commit, remove, or .gitignore them before running the watcher.");
        return Err(msg.into());
    }

    // --- Check 4: Fetch from remote ---
    // Only attempt fetch if a remote is configured.
    let (has_remote, remotes, _) = run_git(&["remote"])?;
    if has_remote && !remotes.trim().is_empty() {
        println!("Fetching from remote...");
        let (ok, _, stderr) = run_git(&["fetch"])?;
        if !ok {
            return Err(format!("Failed to fetch from remote: {}", stderr.trim()).into());
        }
        println!("Fetch complete.");

        // --- Check 5: Ensure HEAD is in sync with the upstream tracking branch ---
        let (ok, upstream, stderr) =
            run_git(&["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])?;
        if !ok {
            let trimmed = stderr.trim();
            if trimmed.contains("no upstream configured")
                || trimmed.contains("does not point to a branch")
            {
                println!(
                    "[WARNING] No upstream tracking branch configured. Skipping sync check.\n\
                     Consider running: git branch --set-upstream-to=origin/<branch>"
                );
                return Ok(());
            }
            return Err(format!(
                "Failed to determine upstream tracking branch: {}",
                trimmed
            )
            .into());
        }
        let upstream = upstream.trim();

        let (_, local_hash, _) = run_git(&["rev-parse", "HEAD"])?;
        let (_, remote_hash, _) = run_git(&["rev-parse", &format!("{}", upstream)])?;
        let local_hash = local_hash.trim();
        let remote_hash = remote_hash.trim();

        if local_hash != remote_hash {
            let (_, ahead_str, _) = run_git(&[
                "rev-list",
                "--count",
                &format!("{}..HEAD", upstream),
            ])?;
            let (_, behind_str, _) = run_git(&[
                "rev-list",
                "--count",
                &format!("HEAD..{}", upstream),
            ])?;
            let ahead: usize = ahead_str.trim().parse().unwrap_or(0);
            let behind: usize = behind_str.trim().parse().unwrap_or(0);

            let mut msg = format!(
                "HEAD is not in sync with upstream '{}':\n",
                upstream
            );
            msg.push_str(&format!("  Local:  {}\n", local_hash));
            msg.push_str(&format!("  Remote: {}\n", remote_hash));
            if ahead > 0 && behind > 0 {
                msg.push_str(&format!(
                    "  Branch has DIVERGED: {} commit(s) ahead, {} commit(s) behind.\n",
                    ahead, behind
                ));
                msg.push_str("  Please rebase or merge to reconcile.");
            } else if ahead > 0 {
                msg.push_str(&format!("  Local is {} commit(s) AHEAD of remote.\n", ahead));
                msg.push_str("  Please push your changes before running the watcher.");
            } else if behind > 0 {
                msg.push_str(&format!(
                    "  Local is {} commit(s) BEHIND remote.\n",
                    behind
                ));
                msg.push_str("  Please pull the latest changes before running the watcher.");
            }
            return Err(msg.into());
        }

        println!("✅ Repository is clean and in sync with '{}'.", upstream);
    } else {
        println!("✅ Repository is clean. No remote configured — skipping sync check.");
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Async File Watcher with 3-Second Debounce ---");

    // --- 0. Pre-flight checks ---
    preflight_checks()?;

    // --- 1. Setup the Path to Watch ---
    let path_to_watch = Path::new(".");
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
