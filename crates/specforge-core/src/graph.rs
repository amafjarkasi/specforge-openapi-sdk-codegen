//! Schema dependency graph visualization.
use std::fmt::Write;
use crate::ir::{Document, Model, Type};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphFormat { Mermaid, Dot }

pub fn generate_graph(doc: &Document, format: GraphFormat) -> String {
    let graph = build_graph(doc);
    match format {
        GraphFormat::Mermaid => render_mermaid(&graph),
        GraphFormat::Dot => render_dot(&graph, &doc.title),
    }
}

#[derive(Debug)]
struct Graph { nodes: Vec<Node>, edges: Vec<Edge> }
#[derive(Debug)]
struct Node { id: String, label: String, shape: NodeShape }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeShape { Default, Diamond }
#[derive(Debug)]
struct Edge { from: String, to: String, label: Option<String> }

fn build_graph(doc: &Document) -> Graph {
    let mut g = Graph { nodes: Vec::new(), edges: Vec::new() };
    for (name, model) in doc.schemas.iter() {
        match model {
            Model::Object(obj) => {
                let shape = if obj.shape_type.is_some() && obj.properties.is_empty() { NodeShape::Diamond } else { NodeShape::Default };
                let label = obj.description.as_deref().map_or_else(|| name.clone(), |d| format!("{name}\n{d}"));
                g.nodes.push(Node { id: name.clone(), label, shape });
                if let Some(ref ty) = obj.shape_type { for d in collect_references(ty) { add_edge(&mut g.edges, name, &d, None); } }
                if let Some(ref base) = obj.base_type { for d in collect_references(base) { add_edge(&mut g.edges, name, &d, Some("extends")); } }
                for prop in &obj.properties { for d in collect_references(&prop.ty) { add_edge(&mut g.edges, name, &d, None); } }
                if let Some(ref ap) = obj.additional_properties { for d in collect_references(ap) { add_edge(&mut g.edges, name, &d, Some("values")); } }
            }
            Model::Enum(e) => { g.nodes.push(Node { id: e.name.clone(), label: format!("{} (enum)", e.name), shape: NodeShape::Default }); }
        }
    }
    g.edges.sort_by(|a, b| (&a.from, &a.to, &a.label).cmp(&(&b.from, &b.to, &b.label)));
    g.edges.dedup_by(|a, b| a.from == b.from && a.to == b.to && a.label == b.label);
    g
}

fn collect_references(ty: &Type) -> Vec<String> {
    match ty {
        Type::Reference { name, .. } => vec![name.clone()],
        Type::Array { item, .. } => collect_references(item),
        Type::Map { value } => collect_references(value),
        Type::Composition(comp) => comp.members.iter().flat_map(collect_references).collect(),
        _ => vec![],
    }
}
fn add_edge(edges: &mut Vec<Edge>, from: &str, to: &str, label: Option<&str>) {
    edges.push(Edge { from: from.to_string(), to: to.to_string(), label: label.map(String::from) });
}
fn mid(name: &str) -> String { format!("N_{}", name.replace(|c: char| !c.is_ascii_alphanumeric() && c != '_', "_")) }
fn mlbl(l: &str) -> String { l.replace('\\', "\\\\").replace('"', "#quot;") }
fn did(name: &str) -> String { format!("\"{}\"", name.replace('"', "\\\"")) }
fn dlbl(l: &str) -> String { l.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\l") }

fn render_mermaid(g: &Graph) -> String {
    let mut o = String::from("graph TD\n");
    for n in &g.nodes {
        let id = mid(&n.id);
        match n.shape {
            NodeShape::Default => writeln!(o, "    {id}[\"{}\"]", mlbl(&n.label)).unwrap(),
            NodeShape::Diamond => writeln!(o, "    {id}{{\"{}\"}}", mlbl(&n.label)).unwrap(),
        }
    }
    for e in &g.edges {
        let f = mid(&e.from);
        let t = mid(&e.to);
        match &e.label {
            Some(l) => writeln!(o, "    {f} -->|{l}| {t}").unwrap(),
            None => writeln!(o, "    {f} --> {t}").unwrap(),
        }
    }
    o
}

fn render_dot(g: &Graph, title: &str) -> String {
    let mut o = format!(
        "digraph \"{title}\" {{\n    rankdir=TB;\n    node [shape=box, style=filled, fillcolor=\"#f0f0f0\"];\n    edge [color=\"#555555\"];\n\n"
    );
    for n in &g.nodes {
        let id = did(&n.id);
        let l = dlbl(&n.label);
        match n.shape {
            NodeShape::Default => writeln!(o, "    {id} [label=\"{l}\"];").unwrap(),
            NodeShape::Diamond => writeln!(
                o,
                "    {id} [label=\"{l}\", shape=diamond, fillcolor=\"#e8e0f0\"];"
            )
            .unwrap(),
        }
    }
    o.push('\n');
    for e in &g.edges {
        let f = did(&e.from);
        let t = did(&e.to);
        match &e.label {
            Some(l) => writeln!(o, "    {f} -> {t} [label=\"{l}\"];").unwrap(),
            None => writeln!(o, "    {f} -> {t};").unwrap(),
        }
    }
    o.push_str("}\n");
    o
}
