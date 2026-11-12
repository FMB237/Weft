use notify::Watcher as NotifyWatcher;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc as std_mpsc;
use std::time::Duration;
use tokio::sync::mpsc;

/// Events emitted by the file watcher.
#[derive(Debug, Clone)]
pub enum WatchEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Deleted(PathBuf),
    Renamed(PathBuf, PathBuf),
}

impl WatchEvent {
    /// The primary path the event refers to (for `Renamed`, the destination).
    pub fn path(&self) -> &Path {
        match self {
            WatchEvent::Created(p) | WatchEvent::Modified(p) | WatchEvent::Deleted(p) => p,
            WatchEvent::Renamed(_, to) => to,
        }
    }
}

/// How long to wait after the first event before emitting, coalescing rapid
/// edits on the same path into a single event.
const DEBOUNCE_WINDOW: Duration = Duration::from_millis(100);

/// Wrapper around `notify` with debouncing via a channel.
///
/// Rapid successive events on the same path (e.g. a save that triggers
/// modify + create) are coalesced into the latest event so consumers aren't
/// flooded with duplicates.
pub struct Watcher {
    rx: mpsc::UnboundedReceiver<WatchEvent>,
    _notify_handle: notify::RecommendedWatcher,
    pending: VecDeque<WatchEvent>,
}

impl std::fmt::Debug for Watcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Watcher")
            .field("pending", &self.pending.len())
            .finish_non_exhaustive()
    }
}

impl Watcher {
    pub fn watch(root: &Path) -> anyhow::Result<(Self, mpsc::UnboundedSender<WatchEvent>)> {
        let (tx, rx) = mpsc::unbounded_channel();
        let tx_clone = tx.clone();

        let mut watcher =
            notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    map_notify_event(&event, &tx_clone);
                }
            })?;

        watcher.watch(root, notify::RecursiveMode::Recursive)?;

        Ok((
            Self {
                rx,
                _notify_handle: watcher,
                pending: VecDeque::new(),
            },
            tx,
        ))
    }

    /// Receive the next debounced watch event.
    ///
    /// Waits for the first raw event, then holds the debounce window open to
    /// collect any stragglers on the same path before returning the latest one.
    pub async fn next_event(&mut self) -> Option<WatchEvent> {
        // Return any event already debounced and buffered.
        if let Some(ev) = self.pending.pop_front() {
            return Some(ev);
        }

        // Wait for the first raw event.
        let first = self.rx.recv().await?;
        self.pending.push_back(first);

        // Debounce window: collect any further events that arrive shortly after.
        let sleep = tokio::time::sleep(DEBOUNCE_WINDOW);
        tokio::pin!(sleep);
        loop {
            tokio::select! {
                () = &mut sleep => break,
                ev = self.rx.recv() => {
                    if let Some(ev) = ev {
                        self.pending.push_back(ev);
                    }
                }
            }
        }

        // Coalesce events on the same path, keeping the latest per path.
        self.coalesce_pending();

        self.pending.pop_front()
    }

    /// Coalesce buffered events so each path appears at most once (latest wins),
    /// preserving first-seen order.
    fn coalesce_pending(&mut self) {
        let mut latest: HashMap<PathBuf, WatchEvent> = HashMap::new();
        let mut order: Vec<PathBuf> = Vec::new();

        for ev in self.pending.drain(..) {
            let path = ev.path().to_path_buf();
            if latest.contains_key(&path) {
                latest.insert(path.clone(), ev);
            } else {
                latest.insert(path.clone(), ev.clone());
                order.push(path);
            }
        }

        self.pending = order
            .into_iter()
            .filter_map(|path| latest.remove(&path))
            .collect();
    }
}

fn map_notify_event(event: &notify::Event, tx: &mpsc::UnboundedSender<WatchEvent>) {
    for path in &event.paths {
        match event.kind {
            notify::EventKind::Create(_) => {
                let _ = tx.send(WatchEvent::Created(path.clone()));
            }
            notify::EventKind::Modify(_) => {
                let _ = tx.send(WatchEvent::Modified(path.clone()));
            }
            notify::EventKind::Remove(_) => {
                let _ = tx.send(WatchEvent::Deleted(path.clone()));
            }
            _ => {}
        }
    }
}

/// A background watcher whose events can be polled from a GUI frame.
///
/// [`Watcher`] is async and tokio-based; this handle owns a dedicated thread
/// running a current-thread tokio runtime and forwards debounced, vault-relative
/// markdown events over a standard channel. Poll it with [`try_recv`] once per
/// frame.
///
/// [`try_recv`]: WatchHandle::try_recv
pub struct WatchHandle {
    rx: std_mpsc::Receiver<WatchEvent>,
    _thread: std::thread::JoinHandle<()>,
}

impl std::fmt::Debug for WatchHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WatchHandle").finish_non_exhaustive()
    }
}

impl WatchHandle {
    /// Start watching `root` on a background thread.
    pub fn start(root: &Path) -> anyhow::Result<Self> {
        let (tx, rx) = std_mpsc::channel();
        let root = root.to_path_buf();

        let thread = std::thread::Builder::new()
            .name("vault-watcher".into())
            .spawn(move || {
                let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_time()
                    .build()
                else {
                    return;
                };
                runtime.block_on(async move {
                    let Ok((mut watcher, _tx)) = Watcher::watch(&root) else {
                        return;
                    };
                    while let Some(event) = watcher.next_event().await {
                        let Some(event) = normalize_event(&root, event) else {
                            continue;
                        };
                        if tx.send(event).is_err() {
                            break;
                        }
                    }
                });
            })?;

        Ok(Self {
            rx,
            _thread: thread,
        })
    }

    /// Return the next pending event without blocking, if any.
    pub fn try_recv(&self) -> Option<WatchEvent> {
        self.rx.try_recv().ok()
    }
}

/// Convert an absolute watcher path to a vault-relative markdown path, dropping
/// events outside the vault, hidden directories, and non-markdown files.
fn normalize_event(root: &Path, event: WatchEvent) -> Option<WatchEvent> {
    let relative = |path: &Path| -> Option<PathBuf> {
        let stripped = path.strip_prefix(root).ok()?;
        if !is_relevant(stripped) {
            return None;
        }
        Some(stripped.to_path_buf())
    };

    match event {
        WatchEvent::Created(path) => relative(&path).map(WatchEvent::Created),
        WatchEvent::Modified(path) => relative(&path).map(WatchEvent::Modified),
        WatchEvent::Deleted(path) => relative(&path).map(WatchEvent::Deleted),
        WatchEvent::Renamed(from, to) => {
            Some(WatchEvent::Renamed(relative(&from)?, relative(&to)?))
        }
    }
}

fn is_relevant(relative: &Path) -> bool {
    if relative
        .components()
        .any(|component| component.as_os_str().to_string_lossy().starts_with('.'))
    {
        return false;
    }
    relative.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_event_debug() {
        let e = WatchEvent::Created(std::path::PathBuf::from("/a.md"));
        assert!(format!("{e:?}").contains("Created"));
    }

    #[test]
    fn watch_event_path_returns_primary_path() {
        let e = WatchEvent::Renamed(
            std::path::PathBuf::from("/a.md"),
            std::path::PathBuf::from("/b.md"),
        );
        assert_eq!(e.path(), std::path::Path::new("/b.md"));
    }

    #[test]
    fn normalize_event_makes_paths_relative_and_filters_unrelated_files() {
        let root = Path::new("/vault");

        let event = normalize_event(
            root,
            WatchEvent::Modified(PathBuf::from("/vault/notes/a.md")),
        );
        assert!(matches!(
            event,
            Some(WatchEvent::Modified(path)) if path == Path::new("notes/a.md")
        ));

        assert!(
            normalize_event(
                root,
                WatchEvent::Modified(PathBuf::from("/vault/.editor/layout.json"))
            )
            .is_none()
        );
        assert!(
            normalize_event(
                root,
                WatchEvent::Modified(PathBuf::from("/vault/image.png"))
            )
            .is_none()
        );
        assert!(
            normalize_event(root, WatchEvent::Modified(PathBuf::from("/elsewhere/a.md"))).is_none()
        );
    }
}
