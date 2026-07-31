//! Performance test suite for specforge generation pipeline.
//!
//! Each test measures end-to-end generation time (parse -> resolve -> emit) and
//! asserts completion within reasonable bounds. Tests also report throughput
//! metrics (schemas/second, operations/second) on success.
//!
//! Run with: `cargo test -p specforge-cli --test performance`
//!
//! Large-spec tests require the external fixtures to be present in the
//! vendored `fixtures/external/` directory.

use specforge_core::{parse_file, resolve};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn parse_resolve_and_report(label: &str, spec_path: &Path) -> (specforge_core::Document, Duration) {
    let start = Instant::now();
    let spec = parse_file(spec_path).unwrap_or_else(|e| panic!("{label}: parse failed: {e}"));
    let parse_time = start.elapsed();

    let t1 = Instant::now();
    let doc = resolve(&spec).unwrap_or_else(|e| panic!("{label}: resolve failed: {e}"));
    let resolve_time = t1.elapsed();

    let total = start.elapsed();
    let schemas = doc.schemas.models.len();
    let ops = doc.operations.len();
    let parse_ms = parse_time.as_secs_f64() * 1000.0;
    let resolve_ms = resolve_time.as_secs_f64() * 1000.0;
    let total_ms = total.as_secs_f64() * 1000.0;

    let schemas_per_sec = if total.as_secs_f64() > 0.0 {
        schemas as f64 / total.as_secs_f64()
    } else {
        0.0
    };
    let ops_per_sec = if total.as_secs_f64() > 0.0 {
        ops as f64 / total.as_secs_f64()
    } else {
        0.0
    };

    eprintln!(
        "[{label}] parse: {parse_ms:.1}ms | resolve: {resolve_ms:.1}ms | total: {total_ms:.1}ms | \
         {schemas} schemas ({schemas_per_sec:.0}/s) | {ops} ops ({ops_per_sec:.0}/s)"
    );

    (doc, total)
}

fn generate_and_report(
    label: &str,
    doc: &specforge_core::Document,
    lang: &str,
) -> (usize, Duration) {
    let start = Instant::now();
    let out = tempfile::tempdir().expect("create temp dir");
    let files = match lang {
        "ts" => {
            let opts = specforge_ts::GeneratorOptions {
                out_dir: out.path().to_path_buf(),
                package_name: Some(format!("@perf/{label}")),
                i18n: None,
            };
            specforge_ts::generate(doc, &opts).expect("{label}: ts emit failed")
        }
        "go" => {
            let opts = specforge_go::GeneratorOptions {
                out_dir: out.path().to_path_buf(),
                module_path: Some(format!("github.com/perf/{label}-go")),
                package_name: None,
                i18n: None,
            };
            specforge_go::generate(doc, &opts).expect("{label}: go emit failed")
        }
        "rust" => {
            let opts = specforge_rust::GeneratorOptions {
                out_dir: out.path().to_path_buf(),
                crate_name: Some(format!("perf_{label}_sdk")),
                i18n: None,
            };
            specforge_rust::generate(doc, &opts).expect("{label}: rust emit failed")
        }
        _ => panic!("unsupported lang: {lang}"),
    };
    let elapsed = start.elapsed();
    let file_count = files.len();
    let ms = elapsed.as_secs_f64() * 1000.0;
    let schemas_per_sec = if elapsed.as_secs_f64() > 0.0 {
        doc.schemas.models.len() as f64 / elapsed.as_secs_f64()
    } else {
        0.0
    };
    eprintln!(
        "[{label}/{lang}] emit: {ms:.1}ms | {file_count} files | \
         schemas/sec: {schemas_per_sec:.0}"
    );
    (file_count, elapsed)
}

// ─── Parse + resolve benchmarks ─────────────────────────────────────────────

#[test]
fn performance_petstore_parse_resolve_in_under_100ms() {
    let path = fixtures_dir().join("petstore.yaml");
    assert!(path.exists(), "petstore fixture missing");
    let (_, elapsed) = parse_resolve_and_report("petstore", &path);
    assert!(
        elapsed < Duration::from_millis(100),
        "petstore parse+resolve took {elapsed:?}, expected < 100ms"
    );
}

#[test]
fn performance_petstore_generates_ts_in_under_100ms() {
    let path = fixtures_dir().join("petstore.yaml");
    let (doc, _) = parse_resolve_and_report("petstore", &path);
    let (files, elapsed) = generate_and_report("petstore", &doc, "ts");
    assert!(files > 5, "expected multiple files, got {files}");
    assert!(
        elapsed < Duration::from_millis(100),
        "petstore ts emit took {elapsed:?}, expected < 100ms"
    );
}

#[test]
fn performance_petstore_generates_go_in_under_100ms() {
    let path = fixtures_dir().join("petstore.yaml");
    let (doc, _) = parse_resolve_and_report("petstore", &path);
    let (files, elapsed) = generate_and_report("petstore", &doc, "go");
    assert!(files > 3, "expected multiple files, got {files}");
    assert!(
        elapsed < Duration::from_millis(100),
        "petstore go emit took {elapsed:?}, expected < 100ms"
    );
}

#[test]
fn performance_petstore_generates_rust_in_under_100ms() {
    let path = fixtures_dir().join("petstore.yaml");
    let (doc, _) = parse_resolve_and_report("petstore", &path);
    let (files, elapsed) = generate_and_report("petstore", &doc, "rust");
    assert!(files > 3, "expected multiple files, got {files}");
    assert!(
        elapsed < Duration::from_millis(100),
        "petstore rust emit took {elapsed:?}, expected < 100ms"
    );
}

// ─── Large spec: GitHub ─────────────────────────────────────────────────────

#[test]
fn performance_github_generates_in_under_5s() {
    let path = fixtures_dir().join("external/github.yaml");
    if !path.exists() {
        eprintln!("skip github perf: fixture not present");
        return;
    }
    let (doc, parse_resolve_time) = parse_resolve_and_report("github", &path);
    assert!(
        parse_resolve_time < Duration::from_secs(10),
        "github parse+resolve took {parse_resolve_time:?}, expected < 10s"
    );
    let schemas = doc.schemas.models.len();
    let ops = doc.operations.len();
    assert!(
        schemas > 100,
        "github should have >100 schemas, got {schemas}"
    );
    assert!(ops > 100, "github should have >100 ops, got {ops}");
}

#[test]
fn performance_github_emit_ts_in_under_10s() {
    let path = fixtures_dir().join("external/github.yaml");
    if !path.exists() {
        eprintln!("skip github ts emit perf: fixture not present");
        return;
    }
    let (doc, _) = parse_resolve_and_report("github", &path);
    let (files, elapsed) = generate_and_report("github", &doc, "ts");
    assert!(
        files > 20,
        "github ts should produce >20 files, got {files}"
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "github ts emit took {elapsed:?}, expected < 10s"
    );
}

#[test]
fn performance_github_emit_go_in_under_10s() {
    let path = fixtures_dir().join("external/github.yaml");
    if !path.exists() {
        eprintln!("skip github go emit perf: fixture not present");
        return;
    }
    let (doc, _) = parse_resolve_and_report("github", &path);
    let (files, elapsed) = generate_and_report("github", &doc, "go");
    assert!(files > 5, "github go should produce >5 files, got {files}");
    assert!(
        elapsed < Duration::from_secs(10),
        "github go emit took {elapsed:?}, expected < 10s"
    );
}

#[test]
fn performance_github_emit_rust_in_under_10s() {
    let path = fixtures_dir().join("external/github.yaml");
    if !path.exists() {
        eprintln!("skip github rust emit perf: fixture not present");
        return;
    }
    let (doc, _) = parse_resolve_and_report("github", &path);
    let (files, elapsed) = generate_and_report("github", &doc, "rust");
    assert!(
        files > 5,
        "github rust should produce >5 files, got {files}"
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "github rust emit took {elapsed:?}, expected < 10s"
    );
}

// ─── Large spec: Stripe ─────────────────────────────────────────────────────

#[test]
fn performance_stripe_generates_in_under_5s() {
    let path = fixtures_dir().join("external/stripe.yaml");
    if !path.exists() {
        eprintln!("skip stripe perf: fixture not present");
        return;
    }
    let (doc, parse_resolve_time) = parse_resolve_and_report("stripe", &path);
    assert!(
        parse_resolve_time < Duration::from_secs(10),
        "stripe parse+resolve took {parse_resolve_time:?}, expected < 10s"
    );
    let schemas = doc.schemas.models.len();
    let ops = doc.operations.len();
    assert!(
        schemas > 100,
        "stripe should have >100 schemas, got {schemas}"
    );
    assert!(ops > 100, "stripe should have >100 ops, got {ops}");
}

#[test]
fn performance_stripe_emit_ts_in_under_10s() {
    let path = fixtures_dir().join("external/stripe.yaml");
    if !path.exists() {
        eprintln!("skip stripe ts emit perf: fixture not present");
        return;
    }
    let (doc, _) = parse_resolve_and_report("stripe", &path);
    let (files, elapsed) = generate_and_report("stripe", &doc, "ts");
    assert!(
        files > 20,
        "stripe ts should produce >20 files, got {files}"
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "stripe ts emit took {elapsed:?}, expected < 10s"
    );
}

#[test]
fn performance_stripe_emit_go_in_under_10s() {
    let path = fixtures_dir().join("external/stripe.yaml");
    if !path.exists() {
        eprintln!("skip stripe go emit perf: fixture not present");
        return;
    }
    let (doc, _) = parse_resolve_and_report("stripe", &path);
    let (files, elapsed) = generate_and_report("stripe", &doc, "go");
    assert!(files > 5, "stripe go should produce >5 files, got {files}");
    assert!(
        elapsed < Duration::from_secs(10),
        "stripe go emit took {elapsed:?}, expected < 10s"
    );
}

#[test]
fn performance_stripe_emit_rust_in_under_10s() {
    let path = fixtures_dir().join("external/stripe.yaml");
    if !path.exists() {
        eprintln!("skip stripe rust emit perf: fixture not present");
        return;
    }
    let (doc, _) = parse_resolve_and_report("stripe", &path);
    let (files, elapsed) = generate_and_report("stripe", &doc, "rust");
    assert!(
        files > 5,
        "stripe rust should produce >5 files, got {files}"
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "stripe rust emit took {elapsed:?}, expected < 10s"
    );
}

// ─── Throughput summary ─────────────────────────────────────────────────────

#[test]
fn performance_throughput_summary() {
    let specs = [
        ("petstore", "petstore.yaml"),
        ("github", "external/github.yaml"),
        ("stripe", "external/stripe.yaml"),
    ];

    eprintln!();
    eprintln!("=== Throughput Summary ===");
    eprintln!(
        "| {:<12} | {:<8} | {:>10} | {:>6} | {:>12} | {:>10} |",
        "Spec", "Lang", "Time", "Files", "Schemas/sec", "Ops/sec"
    );
    eprintln!(
        "| {:<12} | {:<8} | {:>10} | {:>6} | {:>12} | {:>10} |",
        "----", "----", "----", "-----", "-----------", "-------"
    );

    for (label, filename) in &specs {
        let path = fixtures_dir().join(filename);
        if !path.exists() {
            eprintln!(
                "| {:<12} | {:<8} | {:>10} | {:>6} | {:>12} | {:>10} |",
                label, "-", "skip", "-", "-", "-"
            );
            continue;
        }

        let (doc, total_time) = parse_resolve_and_report(label, &path);
        let total_secs = total_time.as_secs_f64();
        let schemas_per_sec = if total_secs > 0.0 {
            doc.schemas.models.len() as f64 / total_secs
        } else {
            0.0
        };
        let ops_per_sec = if total_secs > 0.0 {
            doc.operations.len() as f64 / total_secs
        } else {
            0.0
        };

        for lang in &["ts", "go", "rust"] {
            let (files, emit_time) = generate_and_report(label, &doc, lang);
            let emit_ms = emit_time.as_secs_f64() * 1000.0;
            eprintln!(
                "| {:<12} | {:<8} | {:>8.1}ms | {:>6} | {:>10.0}/s | {:>8.0}/s |",
                label, lang, emit_ms, files, schemas_per_sec, ops_per_sec
            );
        }
    }

    eprintln!();
    eprintln!("=== Done ===");
}
