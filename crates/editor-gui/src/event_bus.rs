//! Cross-panel event bus.
//!
//! Allows panels to communicate without direct coupling. Panels publish
//! events and other panels subscribe to react to them. The bus is processed
//! once per frame by the app, delivering all queued events to every
//! subscriber.

use std::collections::VecDeque;
use std::path::PathBuf;

/// Events that can be published on the bus.
#[derive(Debug, Clone)]
pub enum Event {
    /// A node/file was selected (e.g. clicked in file tree or graph).
    NodeSelected(PathBuf),
    /// A document was opened for editing.
    NodeOpened(PathBuf),
    /// The link graph changed (nodes/edges added/removed).
    GraphChanged,
    /// A document was saved to disk.
    DocumentSaved(PathBuf),
    /// The vault contents changed (files added/removed/renamed).
    VaultChanged,
    /// Theme changed.
    ThemeChanged,
}

/// A callback that handles an event.
type EventHandler = Box<dyn FnMut(&Event)>;

/// Simple in-memory event bus.
///
/// Events are queued with [`publish`] and delivered to all subscribers when
/// [`process`] is called (typically once per frame).
pub struct EventBus {
    queue: VecDeque<Event>,
    subscribers: Vec<EventHandler>,
}

impl std::fmt::Debug for EventBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventBus")
            .field("queue", &self.queue)
            .field("subscribers", &format_args!("{} handlers", self.subscribers.len()))
            .finish()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    /// Create an empty event bus.
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            subscribers: Vec::new(),
        }
    }

    /// Publish an event. It will be delivered to all subscribers on the next
    /// call to [`process`].
    pub fn publish(&mut self, event: Event) {
        self.queue.push_back(event);
    }

    /// Subscribe a handler to receive all published events.
    pub fn subscribe<F>(&mut self, handler: F)
    where
        F: FnMut(&Event) + 'static,
    {
        self.subscribers.push(Box::new(handler));
    }

    /// Deliver all queued events to every subscriber, clearing the queue.
    pub fn process(&mut self) {
        while let Some(event) = self.queue.pop_front() {
            for handler in &mut self.subscribers {
                handler(&event);
            }
        }
    }

    /// Returns the number of pending (undelivered) events.
    pub fn pending(&self) -> usize {
        self.queue.len()
    }

    /// Drain all pending events into a vector, clearing the queue.
    ///
    /// Useful for the app to dispatch events to panels without storing
    /// `FnMut` closures that would capture app state.
    pub fn drain(&mut self) -> Vec<Event> {
        let mut out = Vec::new();
        while let Some(event) = self.queue.pop_front() {
            out.push(event);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    #[test]
    fn event_delivered_to_subscribers() {
        let mut bus = EventBus::new();
        let received = Rc::new(RefCell::new(Vec::new()));
        let received_copy = received.clone();
        bus.subscribe(move |e| {
            received_copy.borrow_mut().push(e.clone());
        });

        bus.publish(Event::NodeSelected(PathBuf::from("a.md")));
        bus.process();

        assert_eq!(received.borrow().len(), 1);
        assert!(matches!(
            received.borrow()[0].clone(),
            Event::NodeSelected(p) if p == PathBuf::from("a.md")
        ));
    }

    #[test]
    fn multiple_subscribers_receive_event() {
        let mut bus = EventBus::new();
        let count1 = Rc::new(RefCell::new(0usize));
        let count1_copy = count1.clone();
        let count2 = Rc::new(RefCell::new(0usize));
        let count2_copy = count2.clone();
        bus.subscribe(move |_| *count1_copy.borrow_mut() += 1);
        bus.subscribe(move |_| *count2_copy.borrow_mut() += 1);

        bus.publish(Event::GraphChanged);
        bus.process();

        assert_eq!(*count1.borrow(), 1);
        assert_eq!(*count2.borrow(), 1);
    }

    #[test]
    fn events_queued_until_process() {
        let mut bus = EventBus::new();
        let received = Rc::new(RefCell::new(Vec::new()));
        let received_copy = received.clone();
        bus.subscribe(move |e| received_copy.borrow_mut().push(e.clone()));

        bus.publish(Event::VaultChanged);
        bus.publish(Event::ThemeChanged);
        assert_eq!(bus.pending(), 2);
        assert!(received.borrow().is_empty());

        bus.process();
        assert_eq!(bus.pending(), 0);
        assert_eq!(received.borrow().len(), 2);
    }
}
