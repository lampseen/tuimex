//! Render and input. This crate draws into a buffer and turns keys into
//! state; it holds no handle on anything that blocks (ADR-006), so the
//! binary owns the terminal and feeds it.

pub mod app;
pub mod calendar;
pub mod command;
pub mod compose;
pub mod editor;
pub mod glyphs;
pub mod help;
pub mod keymap;
pub mod layout;
pub mod list;
pub mod mailbox;
pub mod paint;
pub mod palette;
pub mod reader;
pub mod say;
pub mod span;
pub mod when;
