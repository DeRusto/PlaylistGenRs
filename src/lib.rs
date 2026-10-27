//! PlaylistGenRs: scan media directories for TV shows and write an interleaved M3U playlist.
//!
//! All logic lives here; the CLI, text menu and GUI are thin frontends.

pub mod build;
pub mod cli;
#[cfg(feature = "gui")]
pub mod gui;
pub mod interleave;
pub mod layout;
pub mod rotation;
pub mod scan;
pub mod tui;
