//! Radiance Music's engine: everything that isn't a screen (sources, the player and its parts,
//! effects, lyrics, the library, splitting, servers, updates, …), shared by the GTK app and the
//! Windows app (WinUI 3, through the ffi crate). Issue #45.
//!
//! SPDX-License-Identifier: GPL-3.0-or-later

/// The app's id (GApplication, D-Bus, the desktop file, Flatpak).
pub const APP_ID: &str = "io.github.radianceadm.Music";

/// The engine's version: the computer app's.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod align;
pub mod archive;
pub mod art;
pub mod audio_io;
pub mod beat;
pub mod check;
pub mod controller;
pub mod db;
pub mod effects;
pub mod feedback;
pub mod fx;
pub mod genre;
pub mod home;
pub mod library;
pub mod limiter;
pub mod log;
pub mod lyrics;
pub mod mix;
pub mod one_copy;
pub mod net;
#[cfg(feature = "onnx-split")]
pub mod onnx_split;
pub mod player;
pub mod playing;
pub mod relay;
pub mod report;
pub mod servers;
pub mod session;
pub mod settings;
pub mod setup;
pub mod share;
pub mod sources;
pub mod spleeter;
pub mod split;
pub mod storage;
/// Songs: the core's (core/src/track.rs).
pub use radiance_core::track;
pub mod update;
pub mod youtube;
