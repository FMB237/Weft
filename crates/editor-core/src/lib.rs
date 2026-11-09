#![cfg_attr(
    test,
    expect(clippy::unwrap_used, reason = "tests panic on fixture setup failures")
)]

pub mod clustering;
pub mod document;
pub mod export;
pub mod file_ops;
pub mod graph;
pub mod graph_queries;
pub mod graph_updater;
pub mod index;
pub mod layout;
pub mod layout_store;
pub mod parser;
pub mod plugins;
pub mod vault;
pub mod watcher;
