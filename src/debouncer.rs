// src/debouncer.rs

use crate::event_handler;
use notify::Event;
use std::time::Duration;
use tokio::sync::mpsc::Receiver;
use tokio::time::sleep;

/// The core debouncing logic.
///
/// It waits for a first event, then collects subsequent events that arrive
/// within the `debounce_duration`. Once the "event storm" is over, it
/// processes the entire batch of events at once.
pub async fn debouncer(mut rx: Receiver<Event>, debounce_duration: Duration) {
    let mut accumulated_events: Vec<Event> = Vec::new();

    // The outer loop waits for the first event of a new batch.
    // `recv()` will return `None` if the channel is closed, breaking the loop.
    while let Some(first_event) = rx.recv().await {
        println!("-> Event received. Starting debounce timer...");
        accumulated_events.push(first_event);

        // The inner loop drains any subsequent events that arrive within the
        // debounce duration.
        loop {
            tokio::select! {
                // If another event arrives, add it to the batch. The `select!` loop will
                // then restart, effectively resetting the sleep timer.
                res = rx.recv() => {
                    match res {
                        Some(event) => {
                            println!("-> Event received. Resetting debounce timer...");
                            accumulated_events.push(event);
                        }
                        // Channel closed, break the inner loop to process any remaining events.
                        None => break,
                    }
                }

                // If the timer expires, the event storm is over. Break the inner
                // loop to process the batch.
                _ = sleep(debounce_duration) => {
                    break;
                }
            }
        }

        // Process the accumulated events and reset for the next batch.
        event_handler::handle_events(&accumulated_events);
        accumulated_events.clear();
    }
}
