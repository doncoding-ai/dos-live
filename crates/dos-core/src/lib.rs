//! dos-core — everything Dos Live knows, with no operating-system code in it.
//!
//! The app (Tauri, Windows) fetches Trello, listens and speaks; this crate turns
//! boards into pills, pings and asks, words into intents, and state into
//! sentences. Keeping it pure means it is tested on any machine.

pub mod feed;
pub mod intent;
pub mod relay;
pub mod snapshot;
pub mod speech;
pub mod trello;
pub mod worlds;

pub use snapshot::{build_snapshot, Ask, BoardHealth, BoardLayout, Decision, Ping, Snapshot, Tracker};
pub use worlds::{default_worlds, Rules, WorldDef, WorldState, WorldStatus};

#[cfg(test)]
mod tests;
