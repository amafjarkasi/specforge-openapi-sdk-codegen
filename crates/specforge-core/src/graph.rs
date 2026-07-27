//! Schema dependency graph visualization.
//!
//! Generates Mermaid (`graph TD`) or Graphviz DOT diagrams showing the
//! relationships between named schemas: `$ref` edges, `allOf` composition,
//! `oneOf`/`anyOf` union arms, array items, and map values.

use std::fmt::Write;

use crate::ir::{Composition, Document, Model, Type};

// ─── Public API ──────────────────────────────────────────────────────────────

/// Output format for the dependency graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphFormat {
    /// Mermaid diagram (default). Renderable on GitHub, in Markdown files,
    /// and with the `mmdc` CLI tool.
    Mermaid,
    /// Graphviz DOT format. Renderable with `dot`, `neato`, `fdp`, etc.
    Dot,
}

/// Generate a schema dependency graph from a resolved IR document.
///
/// Returns the graph in the requested format as a `String`.
pub fn generate_graph(doc: &Document, format: GraphFormat) -> String {
    let graph = build_graph(doc);
    match format {
        GraphFormat::Mermaid => render_mermaid(&graph),
        GraphFormat::Dot => render_dot(&graph, &doc.title),
    }
}

// ─── Internal graph model ────────────────────────────────────────────────────

#[derive(Debug)]
struct Graph {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

#[derive(Debug)]
struct Node {
    /// Machine-safe identifier (used in edge references).
    id: String,
    /// Human-readable display label.
    label: String,
    /// Shape hint for DOT rendering.
    shape: NodeShape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeShape {
    /// Default rounded box.
    Default,
    /// Diamond / decision node (used for compositions).
    Diamond,
}

#[derive(Debug)]
struct Edge {
    from: String,
    to: String,
    label: Option<String>,
}

// ─── Graph builder ───────────────────────────────────────────────────────────

fn build_graph(doc: &Document) -> Graph {
    let mut graph = Graph {
        nodes: Vec::new(),
        edges: Vec::new(),
    };

    for (name, model) in doc.schemas.iter() {
        match model {
            Model::Object(obj) => {
                let shape = if obj.shape_type.is_some() && obj.properties.is_empty() {
                    NodeShape::Diamond
                } else {
                    NodeShape::Default
                };
                let label = if let Some(ref desc) = obj.description {
                    format!("{name}\n{desc}")
                } else {
                    name.clone()
                };
                graph.nodes.push(Node {
                    id: name.clone(),
                    label,
                    shape,
                });

                // shape_type edges (allOf / oneOf / anyOf / alias).
                if let Some(ref shape_ty) = obj.shape_type {
                    for dep in collect_references(shape_ty) {
                        add_edge(&mut graph.edges, name, &dep, None);
                    }
                }

                // base_type edge (allOf single-$ref composition).
                if let Some(ref base) = obj.base_type {
                    for dep in collect_references(base) {
                        add_edge(&mut graph.edges, name, &dep, Some("extends"));
                    }
                }

                // Property edges.
                for prop in &obj.properties {
                    for dep in collect_references(&prop.ty) {
                        add_edge(&mut graph.edges, name, &dep, None);
                    }
                }

                // additionalProperties edges.
                if let Some(ref ap) = obj.additional_properties {
                    for dep in collect_references(ap) {
                        add_edge(&mut graph.edges, name, &dep, Some("values"));
                    }
                }
            }
            Model::Enum(e) => {
                let label = format!("{} (enum)", e.name);
                graph.nodes.push(Node {
                    id: e.name.clone(),
                    label,
                    shape: NodeShape::Default,
                });
            }
        }
    }

    deduplicate_edges(&mut graph.edges);
    graph
}

/// Recursively collect named references from a `Type`.
fn collect_references(ty: &Type) -> Vec<String> {
    match ty {
        Type::Reference { name, .. } => vec![name.clone()],
        Type::Array { item, .. } => collect_references(item),
        Type::Map { value } => collect_references(value),
        Type::Composition(comp) => collect_references_from_composition(comp),
        _ => vec![],
    }
}

fn collect_references_from_composition(comp: &Composition) -> Vec<String> {
    comp.members
        .iter()
        .flat_map(collect_references)
        .collect()
}

fn add_edge(edges: &mut Vec<Edge>, from: &str, to: &str, label: Option<&str>) {
    // Only add edges where both endpoints are known schemas.
    edges.push(Edge {
        from: from.to_string(),
        to: to.to_string(),
        label: label.map(String::from),
    });
}

fn deduplicate_edges(edges: &mut Vec<Edge>) {
    edges.sort_by(|a, b| (&a.from, &a.to, &a.label).cmp(&(&b.from, &b.to, &b.label)));
    edges.dedup_by(|a, b| a.from == b.from && a.to == b.to && a.label == b.label);
}

// ─── Mermaid renderer ────────────────────────────────────────────────────────

fn render_mermaid(graph: &Graph) -> String {
    let mut out = String::with_capacity(1024);
    out.push_str("graph TD\n");

    for node in &graph.nodes {
        let id = mermaid_id(&node.id);
        match node.shape {
            NodeShape::Default => {
                writeln!(out, "    {id}[\"{}\"]", mermaid_label(&node.label)).unwrap();
            }
            NodeShape::Diamond => {
                writeln!(out, "    {id}{{\"{}\"}}", mermaid_label(&node.label)).unwrap();
            }
        }
    }

    for edge in &graph.edges {
        let from = mermaid_id(&edge.from);
        let to = mermaid_id(&edge.to);
        match &edge.label {
            Some(label) => {
                writeln!(out, "    {from} -->|{label}| {to}").unwrap();
            }
            None => {
                writeln!(out, "    {from} --> {to}").unwrap();
            }
        }
    }

    out
}

/// Escape a schema name into a safe Mermaid node identifier.
fn mermaid_id(name: &str) -> String {
    // Mermaid node IDs must not contain certain special chars.
    // Wrap in quotes for full safety, but the ID itself needs no escaping.
    format!("N_{}", name.replace(|c: char| !c.is_ascii_alphanumeric() && c != '_', "_"))
}

/// Escape a label for Mermaid quoted node text.
fn mermaid_label(label: &str) -> String {
    label.replace('\\', "\\\\").replace('"', "#quot;")
}

// ─── DOT renderer ────────────────────────────────────────────────────────────

fn render_dot(graph: &Graph, title: &str) -> String {
    let mut out = String::with_capacity(1024);
    writeln!(out, "digraph \"{title}\" {{").unwrap();
    writeln!(out, "    rankdir=TB;").unwrap();
    writeln!(out, "    node [shape=box, style=filled, fillcolor=\"#f0f0f0\"];").unwrap();
    writeln!(out, "    edge [color=\"#555555\"];").unwrap();
    writeln!(out).unwrap();

    for node in &graph.nodes {
        let id = dot_id(&node.id);
        let label = dot_label(&node.label);
        match node.shape {
            NodeShape::Default => {
                writeln!(out, "    {id} [label=\"{label}\"];").unwrap();
            }
            NodeShape::Diamond => {
                writeln!(out, "    {id} [label=\"{label}\", shape=diamond, fillcolor=\"#e8e0f0\"];").unwrap();
            }
        }
    }

    writeln!(out).unwrap();

    for edge in &graph.edges {
        let from = dot_id(&edge.from);
        let to = dot_id(&edge.to);
        match &edge.label {
            Some(label) => {
                writeln!(out, "    {from} -> {to} [label=\"{label}\"];").unwrap();
            }
            None => {
                writeln!(out, "    {from} -> {to};").unwrap();
            }
        }
    }

    writeln!(out, "}}").unwrap();
    out
}

fn dot_id(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\\\""))
}

fn dot_label(label: &str) -> String {
    label.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\l")
}
