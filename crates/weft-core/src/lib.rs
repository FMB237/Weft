#![cfg_attr(
    test,
    expect(clippy::unwrap_used, reason = "tests panic on fixture setup failures")
)]

pub mod clustering;
pub mod crash_recovery;
pub mod daily;
pub mod document;
pub mod export;
pub mod file_ops;
pub mod find;
pub mod fuzzy;
pub mod graph;
pub mod graph_queries;
pub mod graph_updater;
pub mod history;
pub mod index;
pub mod layout;
pub mod layout_store;
pub mod lists;
pub mod nav;
pub mod outline;
pub mod parser;
pub mod plugins;
pub mod properties;
pub mod query;
pub mod tasks;
pub mod templates;
pub mod trash;
pub mod vault;
pub mod views;
pub mod watcher;
pub mod workspace;
