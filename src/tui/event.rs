//! Async event source — merges terminal input with periodic ticks.
//!
//! Uses crossterm's `EventStream` so terminal events and the tick timer are
//! awaited concurrently inside the tokio runtime, never blocking. The loop in
//! [`super::runtime`] selects over this plus the fetch-result channel.

use std::time::Duration;

use crossterm::event::{Event as CtEvent, EventStream, KeyEvent};
use futures::StreamExt;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

/// Normalized events the runtime consumes.
#[derive(Debug, Clone)]
pub enum Event {
    /// A key press.
    Key(KeyEvent),
    /// Terminal resized.
    Resize(u16, u16),
    /// Periodic tick (drives spinner + auto-refresh).
    Tick,
}

/// Spawns a background task that pumps terminal + tick events into a channel,
/// returning the receiver. The task ends when the receiver is dropped.
pub struct EventSource {
    rx: UnboundedReceiver<Event>,
}

impl EventSource {
    pub fn new(tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(run(tx, tick_rate));
        Self { rx }
    }

    /// Await the next event (`None` once the source shuts down).
    pub async fn next(&mut self) -> Option<Event> {
        self.rx.recv().await
    }
}

async fn run(tx: UnboundedSender<Event>, tick_rate: Duration) {
    let mut reader = EventStream::new();
    let mut tick = tokio::time::interval(tick_rate);
    loop {
        tokio::select! {
            _ = tick.tick() => {
                if tx.send(Event::Tick).is_err() {
                    break;
                }
            }
            maybe = reader.next() => {
                match maybe {
                    Some(Ok(ct)) => {
                        if let Some(ev) = translate(ct) {
                            if tx.send(ev).is_err() {
                                break;
                            }
                        }
                    }
                    Some(Err(_)) | None => break,
                }
            }
        }
    }
}

/// Filter crossterm events down to the ones we care about.
fn translate(ct: CtEvent) -> Option<Event> {
    match ct {
        CtEvent::Key(k) if k.kind == crossterm::event::KeyEventKind::Press => Some(Event::Key(k)),
        CtEvent::Resize(w, h) => Some(Event::Resize(w, h)),
        _ => None,
    }
}
