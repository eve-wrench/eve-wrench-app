//! Reading and writing EVE Online settings files: discovery, backups,
//! selective copy, probe formations and portable archives. Every function is
//! blocking; callers run them off the UI thread.

pub mod archive;
pub mod backups;
pub mod config;
pub mod copy;
pub mod esi;
pub mod formations;
pub mod locations;
mod marshal;
pub mod model;
pub mod scan;
pub mod updates;

pub use locations::Locations;
pub use model::*;
