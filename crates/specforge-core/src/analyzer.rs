use crate::ir::{Document, Model, Type};
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, serde::Serialize)]
pub struct AnalysisReport {
    pub total_schemas: usize,
    pub total_operations: usize,
    pub total_size_bytes: usize,
    pub unused_schemas: Vec<String>,
    pub duplicate_schemas: Vec<(String, String)>,
    pub deep_refs: Vec<(String, usize)>,
    pub large_schemas: Vec<(String, usize)>,
    pub recommendations: Vec<String>,
}

pub fn analyze_spec(doc: &Document) -> AnalysisReport {
    let mut report = AnalysisReport {
        total_schemas: doc.schemas.models.len(),
        total_operations: doc.operations.len(),
        total_size_bytes: 0,
        unused_schemas: Vec::new(),
        duplicate_schemas: Vec::new(),
        deep_refs: Vec::new(),
        large_schemas: Vec::new(),
        recommendations: Vec::new(),
    };
    if let Ok(json) = serde_json::to_vec(&doc) { report.total_size_bytes = json.len(); }
    let mut referenced: HashSet<String> = HashSet::new();
    for op in &doc.operations {
        if let Some(body) = &op.request_body { collect_refs_from_type(&body.ty, &mut referenced); }
        for resp in &op.responses { if let Some(ref body) = resp.body { collect_refs_from_type(body, &mut referenced); } }
    }
    for wh in &doc.webhooks {
        if let Some(body) = &wh.request_body { collect_refs_from_type(&body.ty, &mut referenced); }
        for resp in &wh.responses { if let Some(ref body) = resp.body { collect_refs_from_type(body, &mut referenced); } }
    }
    let mut changed = true;
    while changed {
        let current: Vec<String> = referenced.iter().cloned().collect();
        changed = false;
        for name in &current {
            if let Some(model) = doc.schemas.get(name) {
                let mut refs = HashSet::new();
                collect_refs_from_model(model, &mut refs);
                for r in refs { if referenced.insert(r) { changed = true; } }
            }
        }
    }
    for name in doc.schemas.models.keys() {
        if !referenced.contains(name) { report.unused_schemas.push(name.clone()); }
    }
    report.unused_schemas.sort();
    for (name, model) in doc.schemas.iter() {
        if let Model::Object(obj) = model {
            if obj.properties.len() > 20 { report.large_schemas.push((name.clone(), obj.properties.len())); }
        }
    }
    report.large_schemas.sort_by_key(|a| Reverse(a.1));
    let mut fps: HashMap<Vec<String>, Vec<String>> = HashMap::new();
    for (name, model) in doc.schemas.iter() {
        if let Model::Object(obj) = model {
            let mut props: Vec<String> = obj.properties.iter().map(|p| p.name.clone()).collect();
            props.sort();
            fps.entry(props).or_default().push(name.clone());
        }
    }
    let mut dupes: Vec<(String, String)> = Vec::new();
    for group in fps.values() {
        if group.len() > 1 {
            for i in 0..group.len() { for j in (i + 1)..group.len() { dupes.push((group[i].clone(), group[j].clone())); } }
        }
    }
    dupes.sort();
    report.duplicate_schemas = dupes;
    let ref_graph = build_ref_graph(doc);
    let mut memo: HashMap<String, usize> = HashMap::new();
    for name in doc.schemas.models.keys() {
        let depth = ref_depth(name, &ref_graph, &mut memo);
        if depth > 2 { report.deep_refs.push((name.clone(), depth)); }
    }
    report.deep_refs.sort_by_key(|a| Reverse(a.1));
    if !report.unused_schemas.is_empty() {
        report.recommendations.push(format!("Remove {} unused schema(s): {}", report.unused_schemas.len(), report.unused_schemas.join(", ")));
    }
    if !report.duplicate_schemas.is_empty() {
        report.recommendations.push(format!("Found {} pair(s) of schemas with identical property sets.", report.duplicate_schemas.len()));
    }
    for (name, props) in &report.large_schemas {
        report.recommendations.push(format!("Schema '{}' has {} properties. Consider splitting.", name, props));
    }
    for (name, depth) in &report.deep_refs {
        report.recommendations.push(format!("Schema '{}' has deep reference chain (depth {}).", name, depth));
    }
    report
}

fn collect_refs_from_type(ty: &Type, refs: &mut HashSet<String>) {
    match ty {
        Type::Reference { name, .. } => { refs.insert(name.clone()); }
        Type::Array { item, .. } => { collect_refs_from_type(item, refs); }
        Type::Map { value } => { collect_refs_from_type(value, refs); }
        Type::Composition(comp) => { for m in &comp.members { collect_refs_from_type(m, refs); } }
        _ => {}
    }
}

fn collect_refs_from_model(model: &Model, refs: &mut HashSet<String>) {
    match model {
        Model::Object(obj) => {
            for prop in &obj.properties { collect_refs_from_type(&prop.ty, refs); }
            if let Some(ref ap) = obj.additional_properties { collect_refs_from_type(ap, refs); }
            if let Some(ref shape) = obj.shape_type { collect_refs_from_type(shape, refs); }
            if let Some(ref base) = obj.base_type { collect_refs_from_type(base, refs); }
        }
        Model::Enum(_) => {}
    }
}

fn build_ref_graph(doc: &Document) -> HashMap<String, Vec<String>> {
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();
    for (name, model) in doc.schemas.iter() {
        let mut refs = HashSet::new();
        collect_refs_from_model(model, &mut refs);
        graph.insert(name.clone(), refs.into_iter().collect());
    }
    graph
}

fn ref_depth(name: &str, graph: &HashMap<String, Vec<String>>, memo: &mut HashMap<String, usize>) -> usize {
    if let Some(&cached) = memo.get(name) { return cached; }
    memo.insert(name.to_string(), 0);
    let depth = match graph.get(name) {
        Some(targets) if targets.is_empty() => 0,
        Some(targets) => { let mc = targets.iter().map(|t| ref_depth(t, graph, memo)).max().unwrap_or(0); mc + 1 }
        None => 0,
    };
    memo.insert(name.to_string(), depth);
    depth
}
