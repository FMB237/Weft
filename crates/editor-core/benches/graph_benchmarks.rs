//! Benchmarks for link graph operations and layout.

use std::path::PathBuf;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use editor_core::graph::{LinkGraph, NodeId};
use editor_core::layout::{layout_graph, LayoutParams};

fn build_graph(num_nodes: usize) -> LinkGraph {
    let mut graph = LinkGraph::new();
    let mut ids = Vec::with_capacity(num_nodes);
    for i in 0..num_nodes {
        let path = PathBuf::from(format!("node_{}.md", i));
        ids.push(graph.add_node(path, format!("Node {}", i)));
    }
    // Connect each node to a few neighbors to form edges.
    for i in 0..num_nodes.saturating_sub(1) {
        graph.add_edge(ids[i], ids[i + 1], 1.0);
        if i + 2 < num_nodes {
            graph.add_edge(ids[i], ids[i + 2], 1.0);
        }
    }
    graph
}

fn bench_add_nodes(c: &mut Criterion) {
    c.bench_function("graph_add_nodes", |b| {
        b.iter(|| {
            let mut graph = LinkGraph::new();
            for i in 0..1000 {
                let path = PathBuf::from(format!("node_{}.md", i));
                graph.add_node(path, format!("Node {}", i));
            }
            black_box(graph);
        })
    });
}

fn bench_add_edges(c: &mut Criterion) {
    c.bench_function("graph_add_edges", |b| {
        let graph = build_graph(1000);
        b.iter(|| {
            // Rebuild to avoid mutating shared state across iterations.
            let mut g = build_graph(1000);
            let ids: Vec<NodeId> = g.node_ids();
            let n = g.node_count();
            for i in 0..n.saturating_sub(1) {
                g.add_edge(ids[i], ids[(i + 1) % n], 1.0);
            }
            black_box(g);
        })
    });
}

fn bench_node_lookups(c: &mut Criterion) {
    c.bench_function("graph_node_lookups", |b| {
        let mut graph = build_graph(1000);
        let ids: Vec<NodeId> = graph.node_ids();
        b.iter(|| {
            for id in &ids {
                let _ = graph.node(*id);
            }
            black_box(&graph);
        })
    });
}

fn bench_layout_graph(c: &mut Criterion) {
    c.bench_function("layout_graph_100_nodes", |b| {
        let graph = build_graph(100);
        b.iter(|| {
            let layout = layout_graph(&graph, &LayoutParams::default());
            black_box(layout);
        })
    });

    c.bench_function("layout_graph_1000_nodes", |b| {
        let graph = build_graph(1000);
        b.iter(|| {
            let layout = layout_graph(&graph, &LayoutParams::default());
            black_box(layout);
        })
    });
}

criterion_group!(
    benches,
    bench_add_nodes,
    bench_add_edges,
    bench_node_lookups,
    bench_layout_graph
);
criterion_main!(benches);
