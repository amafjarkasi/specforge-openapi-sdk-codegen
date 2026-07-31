//! Criterion benchmarks for specforge parse, resolve, and emit pipelines.
//!
//! Run with: `cargo bench -p specforge-cli`
//!
//! Reports are written to `target/criterion/` (HTML + JSON).

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use specforge_core::{parse_file, resolve};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

fn fixtures_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures"))
}

fn bench_parse_resolve(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_resolve");

    // -- Petstore (tiny, vendored) -------------------------------------------
    let petstore = fixtures_dir().join("petstore.yaml");
    group.bench_function("parse_petstore", |b| {
        b.iter(|| {
            let spec = parse_file(black_box(&petstore)).unwrap();
            black_box(spec);
        })
    });

    group.bench_function("resolve_petstore", |b| {
        let spec = parse_file(&petstore).unwrap();
        b.iter(|| {
            let doc = resolve(black_box(&spec)).unwrap();
            black_box(doc);
        })
    });

    // -- GitHub (large, vendored) --------------------------------------------
    let github = fixtures_dir().join("external/github.yaml");
    group.bench_function("parse_github", |b| {
        b.iter(|| {
            let spec = parse_file(black_box(&github)).unwrap();
            black_box(spec);
        })
    });

    group.bench_function("resolve_github", |b| {
        let spec = parse_file(&github).unwrap();
        b.iter(|| {
            let doc = resolve(black_box(&spec)).unwrap();
            black_box(doc);
        })
    });

    // -- Stripe (large, vendored) --------------------------------------------
    let stripe = fixtures_dir().join("external/stripe.yaml");
    group.bench_function("parse_stripe", |b| {
        b.iter(|| {
            let spec = parse_file(black_box(&stripe)).unwrap();
            black_box(spec);
        })
    });

    group.bench_function("resolve_stripe", |b| {
        let spec = parse_file(&stripe).unwrap();
        b.iter(|| {
            let doc = resolve(black_box(&spec)).unwrap();
            black_box(doc);
        })
    });

    group.finish();
}

fn bench_full_generate(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_generate");
    // Large specs need more measurement time.
    group.sample_size(10);

    // -- Petstore: parse + resolve + emit (TS) -------------------------------
    let petstore = fixtures_dir().join("petstore.yaml");
    group.bench_function("petstore_ts", |b| {
        b.iter_with_large_drop(|| {
            let spec = parse_file(black_box(&petstore)).unwrap();
            let doc = resolve(&spec).unwrap();
            let out = tempfile::tempdir().unwrap();
            let opts = specforge_ts::GeneratorOptions {
                out_dir: out.path().to_path_buf(),
                package_name: Some("@bench/petstore".into()),
                i18n: None,
            };
            let written = specforge_ts::generate(&doc, &opts).unwrap();
            black_box(written);
            black_box(doc);
        })
    });

    group.bench_function("petstore_go", |b| {
        b.iter_with_large_drop(|| {
            let spec = parse_file(black_box(&petstore)).unwrap();
            let doc = resolve(&spec).unwrap();
            let out = tempfile::tempdir().unwrap();
            let opts = specforge_go::GeneratorOptions {
                out_dir: out.path().to_path_buf(),
                module_path: Some("github.com/bench/petstore-go".into()),
                package_name: None,
                i18n: None,
            };
            let written = specforge_go::generate(&doc, &opts).unwrap();
            black_box(written);
            black_box(doc);
        })
    });

    group.bench_function("petstore_rust", |b| {
        b.iter_with_large_drop(|| {
            let spec = parse_file(black_box(&petstore)).unwrap();
            let doc = resolve(&spec).unwrap();
            let out = tempfile::tempdir().unwrap();
            let opts = specforge_rust::GeneratorOptions {
                out_dir: out.path().to_path_buf(),
                crate_name: Some("bench_petstore_sdk".into()),
                i18n: None,
            };
            let written = specforge_rust::generate(&doc, &opts).unwrap();
            black_box(written);
            black_box(doc);
        })
    });

    // -- GitHub: parse + resolve + emit (TS) ---------------------------------
    let github = fixtures_dir().join("external/github.yaml");
    group.bench_function("github_ts", |b| {
        b.iter_with_large_drop(|| {
            let spec = parse_file(black_box(&github)).unwrap();
            let doc = resolve(&spec).unwrap();
            let out = tempfile::tempdir().unwrap();
            let opts = specforge_ts::GeneratorOptions {
                out_dir: out.path().to_path_buf(),
                package_name: Some("@bench/github".into()),
                i18n: None,
            };
            let written = specforge_ts::generate(&doc, &opts).unwrap();
            black_box(written);
            black_box(doc);
        })
    });

    group.bench_function("github_go", |b| {
        b.iter_with_large_drop(|| {
            let spec = parse_file(black_box(&github)).unwrap();
            let doc = resolve(&spec).unwrap();
            let out = tempfile::tempdir().unwrap();
            let opts = specforge_go::GeneratorOptions {
                out_dir: out.path().to_path_buf(),
                module_path: Some("github.com/bench/github-go".into()),
                package_name: None,
                i18n: None,
            };
            let written = specforge_go::generate(&doc, &opts).unwrap();
            black_box(written);
            black_box(doc);
        })
    });

    group.bench_function("github_rust", |b| {
        b.iter_with_large_drop(|| {
            let spec = parse_file(black_box(&github)).unwrap();
            let doc = resolve(&spec).unwrap();
            let out = tempfile::tempdir().unwrap();
            let opts = specforge_rust::GeneratorOptions {
                out_dir: out.path().to_path_buf(),
                crate_name: Some("bench_github_sdk".into()),
                i18n: None,
            };
            let written = specforge_rust::generate(&doc, &opts).unwrap();
            black_box(written);
            black_box(doc);
        })
    });

    // -- Stripe: parse + resolve + emit (TS) ---------------------------------
    let stripe = fixtures_dir().join("external/stripe.yaml");
    group.bench_function("stripe_ts", |b| {
        b.iter_with_large_drop(|| {
            let spec = parse_file(black_box(&stripe)).unwrap();
            let doc = resolve(&spec).unwrap();
            let out = tempfile::tempdir().unwrap();
            let opts = specforge_ts::GeneratorOptions {
                out_dir: out.path().to_path_buf(),
                package_name: Some("@bench/stripe".into()),
                i18n: None,
            };
            let written = specforge_ts::generate(&doc, &opts).unwrap();
            black_box(written);
            black_box(doc);
        })
    });

    group.bench_function("stripe_go", |b| {
        b.iter_with_large_drop(|| {
            let spec = parse_file(black_box(&stripe)).unwrap();
            let doc = resolve(&spec).unwrap();
            let out = tempfile::tempdir().unwrap();
            let opts = specforge_go::GeneratorOptions {
                out_dir: out.path().to_path_buf(),
                module_path: Some("github.com/bench/stripe-go".into()),
                package_name: None,
                i18n: None,
            };
            let written = specforge_go::generate(&doc, &opts).unwrap();
            black_box(written);
            black_box(doc);
        })
    });

    group.bench_function("stripe_rust", |b| {
        b.iter_with_large_drop(|| {
            let spec = parse_file(black_box(&stripe)).unwrap();
            let doc = resolve(&spec).unwrap();
            let out = tempfile::tempdir().unwrap();
            let opts = specforge_rust::GeneratorOptions {
                out_dir: out.path().to_path_buf(),
                crate_name: Some("bench_stripe_sdk".into()),
                i18n: None,
            };
            let written = specforge_rust::generate(&doc, &opts).unwrap();
            black_box(written);
            black_box(doc);
        })
    });

    group.finish();
}

criterion_group!(benches, bench_parse_resolve, bench_full_generate);
criterion_main!(benches);
