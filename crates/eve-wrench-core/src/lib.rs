//! Reading and writing EVE Online settings files: discovery, backups,
//! selective copy, probe formations, drone group presets, confirmation
//! dialogs, window layouts and portable archives. Every function is blocking; callers run them off the UI thread.

pub mod archive;
pub mod backups;
mod catalog;
pub mod config;
pub mod confirmations;
pub mod copy;
pub mod drone_groups;
pub mod esi;
pub mod formations;
pub mod locations;
mod marshal;
pub mod model;
pub mod scan;
pub mod updates;
pub mod window_layout;

pub use locations::Locations;
pub use model::*;
