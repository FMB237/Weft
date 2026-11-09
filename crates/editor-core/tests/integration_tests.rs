//! Integration tests for editor-core crates.
//!
//! These tests exercise cross-module behavior: graph + layout store,
//! parser + graph building, vault CRUD + graph updater, etc.
#![expect(
    clippy::unwrap_used,
    reason = "integration tests panic on fixture setup failures"
)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use editor_core::graph::{LinkGraph, NodeId};
use editor_core::layout_store::LayoutStore;
use editor_core::parser::{ParsedDocument, Wikilink, extract_wikilinks, parse_markdown};
use editor_core::vault::Vault;

fn make_graph_with_nodes(graph: &mut LinkGraph, paths: &[&str]) -> Vec<(NodeId, PathBuf)> {
    let mut ids = Vec::new();
    for p in paths {
        let path = PathBuf::from(p);
        let id = graph.add_node(path.clone(), p.to_string());
        ids.push((id, path));
    }
    ids
}

#[test]
fn link_graph_add_and_lookup_nodes() {
    let mut graph = LinkGraph::new();

    let a = graph.add_node(PathBuf::from("a.md"), "A".to_string());
    let b = graph.add_node(PathBuf::from("b.md"), "B".to_string());
    let c = graph.add_node(PathBuf::from("c.md"), "C".to_string());

    assert_eq!(graph.node_count(), 3);

    let node_a = graph.node(a).expect("node a should exist");
    assert_eq!(node_a.path, PathBuf::from("a.md"));
    assert_eq!(node_a.title, "A");

    let node_by_path = graph
        .node_by_path(PathBuf::from("b.md").as_path())
        .expect("node b by path");
    assert_eq!(node_by_path, b);

    let ids: Vec<NodeId> = graph.node_ids();
    assert_eq!(ids.len(), 3);
    assert!(ids.contains(&a));
    assert!(ids.contains(&b));
    assert!(ids.contains(&c));
}

#[test]
fn link_graph_add_and_remove_edges() {
    let mut graph = LinkGraph::new();
    make_graph_with_nodes(&mut graph, &["a.md", "b.md", "c.md"]);

    let a = graph.node_by_path(PathBuf::from("a.md").as_path()).unwrap();
    let b = graph.node_by_path(PathBuf::from("b.md").as_path()).unwrap();
    let c = graph.node_by_path(PathBuf::from("c.md").as_path()).unwrap();

    graph.add_edge(a, b, 1.0);
    graph.add_edge(b, c, 1.0);
    graph.add_edge(a, c, 1.0);

    assert_eq!(graph.edge_count(), 3);

    let outgoing: Vec<NodeId> = graph
        .outgoing_edges(a)
        .into_iter()
        .map(|e| e.target)
        .collect();
    assert_eq!(outgoing.len(), 2);
    assert!(outgoing.contains(&b));
    assert!(outgoing.contains(&c));

    graph.remove_outgoing_edges(a);
    let outgoing: Vec<NodeId> = graph
        .outgoing_edges(a)
        .into_iter()
        .map(|e| e.target)
        .collect();
    assert!(outgoing.is_empty());
    // Edge b->c should still exist.
    assert_eq!(graph.edge_count(), 1);
}

#[test]
fn link_graph_remove_node_removes_incident_edges() {
    let mut graph = LinkGraph::new();
    make_graph_with_nodes(&mut graph, &["a.md", "b.md", "c.md"]);

    let a = graph.node_by_path(PathBuf::from("a.md").as_path()).unwrap();
    let b = graph.node_by_path(PathBuf::from("b.md").as_path()).unwrap();
    let c = graph.node_by_path(PathBuf::from("c.md").as_path()).unwrap();

    graph.add_edge(a, b, 1.0);
    graph.add_edge(b, c, 1.0);
    graph.add_edge(c, a, 1.0);

    graph.remove_node(b);

    assert_eq!(graph.node_count(), 2);
    assert!(graph.node(b).is_none());
    assert!(
        graph
            .node_by_path(PathBuf::from("b.md").as_path())
            .is_none()
    );

    // All edges incident to b should be gone; c->a should remain.
    assert_eq!(graph.edge_count(), 1);
}

#[test]
fn link_graph_clone_preserves_nodes_and_edges() {
    let mut graph = LinkGraph::new();
    make_graph_with_nodes(&mut graph, &["a.md", "b.md"]);

    let a = graph.node_by_path(PathBuf::from("a.md").as_path()).unwrap();
    let b = graph.node_by_path(PathBuf::from("b.md").as_path()).unwrap();
    graph.add_edge(a, b, 1.0);

    let cloned = graph.clone();
    assert_eq!(cloned.node_count(), 2);
    assert_eq!(cloned.edge_count(), 1);
    let nodes: Vec<NodeId> = cloned.node_ids();
    assert_eq!(nodes.len(), 2);
}

#[test]
fn layout_store_set_and_get_position() {
    let mut store = LayoutStore::new();

    let path_a = PathBuf::from("a.md");
    let path_b = PathBuf::from("b.md");

    assert!(store.position(Path::new("a.md")).is_none());

    store.set_position(path_a.clone(), egui::Pos2::new(10.0, 20.0));
    store.set_position(path_b, egui::Pos2::new(30.0, 40.0));

    assert_eq!(
        store.position(Path::new("a.md")),
        Some(egui::Pos2::new(10.0, 20.0))
    );
    assert_eq!(
        store.position(Path::new("b.md")),
        Some(egui::Pos2::new(30.0, 40.0))
    );

    let positions: Vec<(PathBuf, egui::Pos2)> = store.positions();
    let pos_map: HashMap<&Path, egui::Pos2> = positions
        .iter()
        .map(|(p, pos)| (p.as_path(), *pos))
        .collect();
    assert_eq!(pos_map.len(), 2);
    assert_eq!(
        pos_map.get(Path::new("a.md")),
        Some(&egui::Pos2::new(10.0, 20.0))
    );
    assert_eq!(
        pos_map.get(Path::new("b.md")),
        Some(&egui::Pos2::new(30.0, 40.0))
    );

    store.remove_position(&path_a);
    assert!(store.position(Path::new("a.md")).is_none());
    assert_eq!(
        store.position(Path::new("b.md")),
        Some(egui::Pos2::new(30.0, 40.0))
    );
}

#[test]
fn layout_store_json_round_trip() {
    let mut store = LayoutStore::new();
    store.set_position(PathBuf::from("a.md"), egui::Pos2::new(1.5, 2.5));
    store.set_position(PathBuf::from("sub/b.md"), egui::Pos2::new(-3.0, 4.0));

    let json = store.to_json().expect("serialize should succeed");

    let restored = LayoutStore::from_json(json.as_str()).expect("deserialize should succeed");
    assert_eq!(
        restored.position(Path::new("a.md")),
        Some(egui::Pos2::new(1.5, 2.5))
    );
    assert_eq!(
        restored.position(Path::new("sub/b.md")),
        Some(egui::Pos2::new(-3.0, 4.0))
    );
}

#[test]
fn layout_store_apply_to_graph_updates_node_positions() {
    let mut graph = LinkGraph::new();
    make_graph_with_nodes(&mut graph, &["a.md", "b.md"]);

    let mut store = LayoutStore::new();
    store.set_position(PathBuf::from("a.md"), egui::Pos2::new(100.0, 200.0));
    store.set_position(PathBuf::from("b.md"), egui::Pos2::new(300.0, 400.0));

    store.apply_to_graph(&mut graph);

    let a = graph.node_by_path(PathBuf::from("a.md").as_path()).unwrap();
    assert_eq!(
        graph.node(a).unwrap().position,
        Some(egui::Pos2::new(100.0, 200.0))
    );

    let b = graph.node_by_path(PathBuf::from("b.md").as_path()).unwrap();
    assert_eq!(
        graph.node(b).unwrap().position,
        Some(egui::Pos2::new(300.0, 400.0))
    );
}

#[test]
fn parser_extracts_wikilinks() {
    let doc = "Hello [[link to another|alias]] and [[simple link]].";
    let links: Vec<_> = extract_wikilinks(doc);

    assert_eq!(links.len(), 2);

    let first = links.first().unwrap();
    assert_eq!(first.target, "link to another");
    assert_eq!(first.alias.as_deref(), Some("alias"));

    let second = links.get(1).unwrap();
    assert_eq!(second.target, "simple link");
    assert!(second.alias.is_none());
}

#[test]
fn parser_parse_markdown_returns_title_and_links() {
    let content = "---\ntitle = \"My Page\"\ntags = [\"rust\", \"notes\"]\n---\n\n# My Page\n\nSee [[related]] for more.\n";
    let parsed: ParsedDocument = parse_markdown(Path::new("page.md"), content);

    assert_eq!(parsed.frontmatter.title.as_deref(), Some("My Page"));
    assert_eq!(
        parsed.frontmatter.tags,
        vec!["rust".to_string(), "notes".to_string()]
    );

    let links: Vec<Wikilink> = extract_wikilinks(&parsed.html);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target, "related");
}

#[test]
fn vault_crud_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let vault_path = tmp.path().to_path_buf();
    let vault = Vault::open(&vault_path).expect("vault should open");

    // Write files.
    vault.write(Path::new("a.md"), "# A\n").expect("write a.md");
    vault
        .write(Path::new("sub/b.md"), "# B\n")
        .expect("write sub/b.md");

    let files: Vec<PathBuf> = vault.list_files().expect("list files");
    assert!(files.iter().any(|f| f.ends_with("a.md")));
    assert!(files.iter().any(|f| f.ends_with("b.md")));
    assert!(files.iter().any(|f| f.ends_with("sub/b.md")));

    let content = vault.read(Path::new("a.md")).expect("read a.md");
    assert_eq!(content, "# A\n");

    // Delete one file and confirm it's gone.
    vault
        .delete(Path::new("sub/b.md"))
        .expect("delete sub/b.md");
    let files_after: Vec<PathBuf> = vault.list_files().expect("list files again");
    assert!(!files_after.iter().any(|f| f.ends_with("sub/b.md")));
}

#[test]
fn vault_resolve_returns_full_path() {
    let tmp = tempfile::tempdir().unwrap();
    let vault_path = tmp.path().to_path_buf();
    let vault = Vault::open(&vault_path).expect("vault should open");

    vault
        .write(Path::new("nested/deep.md"), "x\n")
        .expect("write nested/deep.md");

    let resolved = vault.resolve(Path::new("nested/deep.md"));
    assert_eq!(resolved, vault_path.join("nested/deep.md"));
}

#[test]
fn plugin_dispatch_notifies_all_plugins() {
    use editor_core::plugins::{PluginEvent, PluginManager};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct CountingPlugin {
        name: &'static str,
        counter: Arc<AtomicUsize>,
    }

    impl editor_core::plugins::Plugin for CountingPlugin {
        fn name(&self) -> &str {
            self.name
        }
        fn on_event(
            &mut self,
            event: &editor_core::plugins::PluginEvent,
            _manager: &mut editor_core::plugins::PluginManager,
        ) -> anyhow::Result<()> {
            if matches!(event, PluginEvent::VaultOpened(_)) {
                self.counter.fetch_add(1, Ordering::Relaxed);
            }
            Ok(())
        }
    }

    let counter_a = Arc::new(AtomicUsize::new(0));
    let counter_b = Arc::new(AtomicUsize::new(0));
    let observed_a = Arc::clone(&counter_a);
    let observed_b = Arc::clone(&counter_b);

    let mut mgr = PluginManager::new();
    mgr.register(Box::new(CountingPlugin {
        name: "first",
        counter: counter_a,
    }))
    .unwrap();
    mgr.register(Box::new(CountingPlugin {
        name: "second",
        counter: counter_b,
    }))
    .unwrap();

    mgr.dispatch(&PluginEvent::VaultOpened(PathBuf::from("/vault")))
        .unwrap();

    assert_eq!(observed_a.load(Ordering::Relaxed), 1);
    assert_eq!(observed_b.load(Ordering::Relaxed), 1);
    assert_eq!(mgr.plugin_names(), vec!["first", "second"]);
}

#[test]
fn plugin_graph_and_layout_cross_module() {
    use editor_core::graph::LinkGraph;
    use editor_core::layout::{LayoutParams, layout_graph};
    use editor_core::layout_store::LayoutStore;
    use editor_core::plugins::{PluginEvent, PluginManager};

    // Build a graph, lay it out, persist positions, then notify plugins.
    let mut graph = LinkGraph::new();
    let a = graph.add_node("/a.md".into(), "A".into());
    let b = graph.add_node("/b.md".into(), "B".into());
    graph.add_edge(a, b, 1.0);

    let layout = layout_graph(&graph, &LayoutParams::default());
    assert_eq!(layout.positions.len(), 2);

    let mut store = LayoutStore::new();
    for (id, pos) in &layout.positions {
        let node = graph.node(*id).unwrap();
        store.set_position(node.path.clone(), *pos);
    }
    store.apply_to_graph(&mut graph);

    let mut mgr = PluginManager::new();
    mgr.dispatch(&PluginEvent::VaultOpened(PathBuf::from("/vault")))
        .unwrap();
    assert_eq!(mgr.plugin_count(), 0);
    assert!(graph.node(a).unwrap().position.is_some());
    assert!(graph.node(b).unwrap().position.is_some());
}
