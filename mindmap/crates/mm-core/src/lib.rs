//! Document model, commands, undo/redo. No UI, no I/O, no unsafe.
#![forbid(unsafe_code)]

pub mod command;
pub mod doc;
pub mod editor;
pub mod outline;
pub mod templates;

pub use command::{Command, CoreError};
pub use doc::{DiagramMode, Document, Node, NodeId, Shape, Subtree};
pub use editor::Editor;
