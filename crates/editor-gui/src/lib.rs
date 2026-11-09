#![cfg_attr(
    test,
    expect(clippy::unwrap_used, reason = "tests panic on fixture setup failures")
)]
#![expect(
    clippy::print_stderr,
    reason = "GUI has no logging backend until the M1 logging pass"
)]

pub mod app;
pub mod event_bus;
pub mod panels;
pub mod settings;
pub mod theme;
pub mod widgets;
