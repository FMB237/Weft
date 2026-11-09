//! Integration tests for the cross-panel event bus.
//!
//! These tests verify pub/sub behavior across panels: publishing events,
//! subscribing handlers, delivery on `process`, queueing semantics,
//! `drain`/`pending` counts, and that different event types are delivered
//! to all subscribers without cross-contamination.
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use editor_gui::event_bus::{Event, EventBus};

fn selected(path: &str) -> Event {
    Event::NodeSelected(PathBuf::from(path))
}

#[test]
fn event_delivered_to_subscribers_on_process() {
    let mut bus = EventBus::new();
    let received = Rc::new(RefCell::new(Vec::new()));
    let received_copy = Rc::clone(&received);

    bus.subscribe(move |e| {
        received_copy.borrow_mut().push(e.clone());
    });

    // Nothing delivered until process() is called.
    bus.publish(selected("a.md"));
    assert_eq!(bus.pending(), 1);
    assert!(received.borrow().is_empty());

    bus.process();
    assert_eq!(bus.pending(), 0);
    assert_eq!(received.borrow().len(), 1);
    assert!(matches!(
        received.borrow()[0].clone(),
        Event::NodeSelected(p) if p.as_path() == std::path::Path::new("a.md")
    ));
}

#[test]
fn multiple_subscribers_each_receive_every_event() {
    let mut bus = EventBus::new();
    let count1 = Rc::new(RefCell::new(0usize));
    let count1_copy = Rc::clone(&count1);
    let count2 = Rc::new(RefCell::new(0usize));
    let count2_copy = Rc::clone(&count2);
    let count3 = Rc::new(RefCell::new(0usize));
    let count3_copy = Rc::clone(&count3);

    bus.subscribe(move |_| *count1_copy.borrow_mut() += 1);
    bus.subscribe(move |_| *count2_copy.borrow_mut() += 1);
    bus.subscribe(move |_| *count3_copy.borrow_mut() += 1);

    bus.publish(Event::GraphChanged);
    bus.publish(Event::VaultChanged);
    bus.publish(Event::ThemeChanged);
    bus.process();

    assert_eq!(*count1.borrow(), 3);
    assert_eq!(*count2.borrow(), 3);
    assert_eq!(*count3.borrow(), 3);
}

#[test]
fn drain_returns_queued_events_and_clears_queue() {
    let mut bus = EventBus::new();
    bus.publish(selected("a.md"));
    bus.publish(Event::GraphChanged);
    bus.publish(selected("b.md"));
    assert_eq!(bus.pending(), 3);

    let drained: Vec<Event> = bus.drain();
    assert_eq!(drained.len(), 3);
    assert_eq!(bus.pending(), 0);

    // Queue is empty after drain; nothing is delivered twice.
    bus.drain(); // should be empty
    bus.process();
    assert!(
        matches!(&drained[0], Event::NodeSelected(p) if p.as_path() == std::path::Path::new("a.md"))
    );
    assert!(matches!(&drained[1], Event::GraphChanged));
    assert!(
        matches!(&drained[2], Event::NodeSelected(p) if p.as_path() == std::path::Path::new("b.md"))
    );
}

#[test]
fn different_event_types_delivered_independently() {
    let mut bus = EventBus::new();
    let graph_changed = Rc::new(RefCell::new(0usize));
    let graph_changed_copy = Rc::clone(&graph_changed);
    let node_selected = Rc::new(RefCell::new(Vec::new()));
    let node_selected_copy = Rc::clone(&node_selected);

    bus.subscribe(move |e| {
        if matches!(e, Event::GraphChanged) {
            *graph_changed_copy.borrow_mut() += 1;
        }
    });
    bus.subscribe(move |e| {
        if let Event::NodeSelected(p) = e {
            node_selected_copy.borrow_mut().push(p.clone());
        }
    });

    bus.publish(Event::GraphChanged);
    bus.publish(selected("x.md"));
    bus.publish(Event::GraphChanged);
    bus.publish(Event::VaultChanged);
    bus.process();

    assert_eq!(*graph_changed.borrow(), 2);
    let selected_paths: Vec<PathBuf> = node_selected.borrow().clone();
    assert_eq!(selected_paths, vec![PathBuf::from("x.md")]);
}

#[test]
fn subscriptions_persist_across_multiple_process_calls() {
    let mut bus = EventBus::new();
    let count = Rc::new(RefCell::new(0usize));
    let count_copy = Rc::clone(&count);
    bus.subscribe(move |_| *count_copy.borrow_mut() += 1);

    bus.publish(Event::ThemeChanged);
    bus.process();
    assert_eq!(*count.borrow(), 1);

    bus.publish(Event::DocumentSaved(PathBuf::from("c.md")));
    bus.process();
    assert_eq!(*count.borrow(), 2);
}
