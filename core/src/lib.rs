//! Radiance Music's core: the behaviour that needs no GLib and no GStreamer (songs, lyrics, …),
//! shared by the engine, the GTK app and the Windows app, and one day the phone's (issue #45).
//!
//! SPDX-License-Identifier: GPL-3.0-or-later

pub mod library;
pub mod lyrics;
pub mod queue;
pub mod search;
pub mod sing;
pub mod track;
pub mod trouble;
pub mod words;
