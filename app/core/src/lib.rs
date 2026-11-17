//! NewsQueue engine. Everything here is free of UI and Win32 code so it builds
//! on Windows and Linux; front ends (the egui desktop app, later the web
//! server) depend on it.

pub mod container;
pub mod download;
pub mod model;
pub mod par2;
pub mod rss;
pub mod unrar;
