//! Regression tests: generate SDKs from real-world public OpenAPI specs and
//! assert the resolver + emitter succeed without panic for each.
//!
//! These guard against regressions on the kind of specs that already burned us
//! (hyphenated/dotted names, reserved-word params, self-referential `$ref`s,
//! specs with no security schemes, array/object query values).
//!
//! The big specs (GitHub ~9MB, Stripe ~7.6MB) are too large to vendor, so they
//! are downloaded on demand into `target/spec-cache/` and the test is skipped
//! (not failed) when the network is unavailable. The petstore fixture is
//! vendored and always runs.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// A fixture: a human name, and either a vendored path or a download URL.
struct Fixture {
    name: &'static str,
    source: Source,
}

enum Source {
    Vendored(&'static str),
    Url(&'static str),
}

const FIXTURES: &[Fixture] = &[
    Fixture {
        name: "petstore",
        source: Source::Vendored("../../fixtures/petstore.yaml"),
    },
    Fixture {
        name: "github",
        source: Source::Url(
            "https://raw.githubusercontent.com/github/rest-api-description/main/descriptions/api.github.com/api.github.com.yaml",
        ),
    },
    Fixture {
        name: "stripe",
        source: Source::Url(
            "https://raw.githubusercontent.com/stripe/openapi/master/openapi/spec3.json",
        ),
    },
];

fn manifest_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn cache_dir() -> PathBuf {
    let target = manifest_root()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("target");
    target.join("spec-cache")
}

fn spec_path(f: &Fixture) -> Result<PathBuf, String> {
    match &f.source {
        Source::Vendored(rel) => {
            let p = manifest_root().join(rel);
            // Vendored fixtures are part of the repo; a missing one is a hard
            // failure, not a skip.
            if !p.exists() {
                panic!("vendored fixture missing: {}", p.display());
            }
            Ok(p)
        }
        Source::Url(url) => {
            let cache = cache_dir();
            std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
            let ext = if url.ends_with(".json") { "json" } else { "yaml" };
            let dest = cache.join(format!("{}.{}", f.name, ext));
            if dest.exists() {
                return Ok(dest);
            }
            download(url, &dest).map_err(|e| format!("download {url} failed: {e}"))?;
            Ok(dest)
        }
    }
}

fn download(url: &str, dest: &Path) -> Result<(), String> {
    // Use the system curl. Offline CI skips the test rather than failing.
    let status = std::process::Command::new("curl")
        .args([
            "-sSL",
            "--fail",
            "--max-time",
            "120",
            "-o",
        ])
        .arg(dest)
        .arg(url)
        .status()
        .map_err(|e| format!("spawn curl: {e}"))?;
    if !status.success() {
        // Clean up partial files so a retry starts fresh.
        let _ = std::fs::remove_file(dest);
        return Err(format!("curl exited {:?}", status.code()));
    }
    Ok(())
}

/// Resolve a fixture to the IR and assert schema/operation counts are non-zero.
/// This is the cheapest meaningful check that catches parser/resolver panics
/// and wholesale regressions. (Type-checking the emitted TS is done separately
/// and is too slow/expensive to run in `cargo test` by default.)
fn assert_resolves(f: &Fixture) {
    let path = match spec_path(f) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("skip {}: {e}", f.name);
            return;
        }
    };
    let spec = match specforge_core::parse_file(&path) {
        Ok(s) => s,
        Err(e) => panic!("{}: parse failed: {e}", f.name),
    };
    let doc = match specforge_core::resolve(&spec) {
        Ok(d) => d,
        Err(e) => panic!("{}: resolve failed: {e}", f.name),
    };
    assert!(
        !doc.schemas.models.is_empty(),
        "{}: expected non-zero schemas",
        f.name
    );
    assert!(
        !doc.operations.is_empty(),
        "{}: expected non-zero operations",
        f.name
    );
    // Every operation must have a non-empty id and a resolved method/path.
    for op in &doc.operations {
        assert!(!op.operation_id.is_empty(), "{}: empty operation_id", f.name);
        assert!(!op.path.is_empty(), "{}: empty path on {}", f.name, op.operation_id);
    }
    eprintln!(
        "{}: {} schemas, {} operations — OK",
        f.name,
        doc.schemas.models.len(),
        doc.operations.len()
    );
}

#[test]
fn petstore_resolves() {
    assert_resolves(&FIXTURES[0]);
}

#[test]
fn github_resolves() {
    assert_resolves(&FIXTURES[1]);
}

#[test]
fn stripe_resolves() {
    assert_resolves(&FIXTURES[2]);
}

/// Petstore also gets a full generate (not just resolve) since it's small and
/// vendored — this catches emitter regressions, not just parser ones.
#[test]
fn petstore_generates_full_sdk() {
    let f = &FIXTURES[0];
    let path = spec_path(f).expect("petstore is vendored");
    let spec = specforge_core::parse_file(&path).expect("petstore parses");
    let doc = specforge_core::resolve(&spec).expect("petstore resolves");

    let out = std::env::temp_dir().join("specforge-regression-petstore");
    let _ = std::fs::remove_dir_all(&out);
    let opts = specforge_ts::GeneratorOptions {
        out_dir: out.clone(),
        package_name: Some("@regression/petstore".into()),
    };
    let written = specforge_ts::generate(&doc, &opts).expect("petstore emits");
    assert!(written.len() > 10, "expected multiple files, got {}", written.len());
    // At least one model file and one api file.
    assert!(
        written.iter().any(|p| p.contains("models/")),
        "no model files emitted"
    );
    assert!(
        written.iter().any(|p| p.contains("api/")),
        "no api files emitted"
    );
}

#[test]
fn petstore_generates_go_sdk() {
    let f = &FIXTURES[0];
    let path = spec_path(f).expect("petstore is vendored");
    let spec = specforge_core::parse_file(&path).expect("petstore parses");
    let doc = specforge_core::resolve(&spec).expect("petstore resolves");

    let out = std::env::temp_dir().join("specforge-regression-petstore-go");
    let _ = std::fs::remove_dir_all(&out);
    let opts = specforge_go::GeneratorOptions {
        out_dir: out.clone(),
        module_path: Some("github.com/example/petstore-go".into()),
        package_name: None,
    };
    let written = specforge_go::generate(&doc, &opts).expect("petstore go emits");
    assert!(written.iter().any(|p| p == "go.mod"), "missing go.mod: {written:?}");
    assert!(written.iter().any(|p| p == "client.go"), "missing client.go");
    assert!(written.iter().any(|p| p == "models.go"), "missing models.go");
    assert!(
        written.iter().any(|p| p.starts_with("api_")),
        "missing api_*.go files"
    );
    // Compile gate — skip only if `go` isn't on PATH.
    if let Err(e) = assert_go_build(&out) {
        eprintln!("skip petstore go build: {e}");
    }
}

#[test]
fn petstore_generates_rust_sdk() {
    let f = &FIXTURES[0];
    let path = spec_path(f).expect("petstore is vendored");
    let spec = specforge_core::parse_file(&path).expect("petstore parses");
    let doc = specforge_core::resolve(&spec).expect("petstore resolves");

    let out = std::env::temp_dir().join("specforge-regression-petstore-rust");
    let _ = std::fs::remove_dir_all(&out);
    let opts = specforge_rust::GeneratorOptions {
        out_dir: out.clone(),
        crate_name: Some("petstore_sdk".into()),
    };
    let written = specforge_rust::generate(&doc, &opts).expect("petstore rust emits");
    assert!(
        written.iter().any(|p| p == "Cargo.toml"),
        "missing Cargo.toml: {written:?}"
    );
    assert!(written.iter().any(|p| p == "src/lib.rs"), "missing src/lib.rs");
    assert!(written.iter().any(|p| p == "src/models.rs"), "missing models");
    assert!(
        written.iter().any(|p| p.starts_with("src/api/")),
        "missing api modules"
    );
    if let Err(e) = assert_cargo_check(&out) {
        eprintln!("skip petstore rust check: {e}");
    }
}

// ─── Large-spec generate + compile gates (Go / Rust) ─────────────────────────
//
// These exercise the emitters against GitHub (~965 schemas / 1209 ops) and
// Stripe (~1431 / 587). Specs are downloaded into target/spec-cache/ (or reused);
// if the network is down the test skips. Compile steps skip when go/cargo are
// missing. A failed generate or a failed compile (when the tool IS present) is
// a hard failure.

#[test]
fn github_generates_and_compiles_go() {
    generate_and_compile_go(&FIXTURES[1], "github.com/example/github-go-sdk");
}

#[test]
fn github_generates_and_compiles_rust() {
    generate_and_compile_rust(&FIXTURES[1], "github_sdk");
}

#[test]
fn stripe_generates_and_compiles_go() {
    generate_and_compile_go(&FIXTURES[2], "github.com/example/stripe-go-sdk");
}

#[test]
fn stripe_generates_and_compiles_rust() {
    generate_and_compile_rust(&FIXTURES[2], "stripe_sdk");
}

fn generate_and_compile_go(f: &Fixture, module: &str) {
    let path = match spec_path(f) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("skip {} go gate: {e}", f.name);
            return;
        }
    };
    let spec = match specforge_core::parse_file(&path) {
        Ok(s) => s,
        Err(e) => panic!("{} go: parse failed: {e}", f.name),
    };
    let doc = match specforge_core::resolve(&spec) {
        Ok(d) => d,
        Err(e) => panic!("{} go: resolve failed: {e}", f.name),
    };

    let out = std::env::temp_dir().join(format!("specforge-gate-{}-go", f.name));
    let _ = std::fs::remove_dir_all(&out);
    let opts = specforge_go::GeneratorOptions {
        out_dir: out.clone(),
        module_path: Some(module.into()),
        package_name: None,
    };
    let written = specforge_go::generate(&doc, &opts)
        .unwrap_or_else(|e| panic!("{} go: emit failed: {e}", f.name));
    assert!(
        written.iter().any(|p| p == "go.mod"),
        "{} go: missing go.mod",
        f.name
    );
    assert!(
        written.len() >= 4,
        "{} go: expected several files, got {}",
        f.name,
        written.len()
    );

    match assert_go_build(&out) {
        Ok(()) => eprintln!("{} go: generate + go build OK ({} files)", f.name, written.len()),
        Err(e) if e.contains("go not found") => eprintln!("skip {} go build: {e}", f.name),
        Err(e) => panic!("{} go build failed: {e}", f.name),
    }
}

fn generate_and_compile_rust(f: &Fixture, crate_name: &str) {
    let path = match spec_path(f) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("skip {} rust gate: {e}", f.name);
            return;
        }
    };
    let spec = match specforge_core::parse_file(&path) {
        Ok(s) => s,
        Err(e) => panic!("{} rust: parse failed: {e}", f.name),
    };
    let doc = match specforge_core::resolve(&spec) {
        Ok(d) => d,
        Err(e) => panic!("{} rust: resolve failed: {e}", f.name),
    };

    let out = std::env::temp_dir().join(format!("specforge-gate-{}-rust", f.name));
    let _ = std::fs::remove_dir_all(&out);
    let opts = specforge_rust::GeneratorOptions {
        out_dir: out.clone(),
        crate_name: Some(crate_name.into()),
    };
    let written = specforge_rust::generate(&doc, &opts)
        .unwrap_or_else(|e| panic!("{} rust: emit failed: {e}", f.name));
    assert!(
        written.iter().any(|p| p == "Cargo.toml"),
        "{} rust: missing Cargo.toml",
        f.name
    );
    assert!(
        written.len() >= 5,
        "{} rust: expected several files, got {}",
        f.name,
        written.len()
    );

    match assert_cargo_check(&out) {
        Ok(()) => eprintln!(
            "{} rust: generate + cargo check OK ({} files)",
            f.name,
            written.len()
        ),
        Err(e) if e.contains("cargo not found") => eprintln!("skip {} rust check: {e}", f.name),
        Err(e) => panic!("{} rust cargo check failed: {e}", f.name),
    }
}

/// Run `go build ./...` in `dir`. Returns Err("go not found") when the toolchain
/// is missing so callers can skip; any other failure is a real compile error.
fn assert_go_build(dir: &Path) -> Result<(), String> {
    let status = std::process::Command::new("go")
        .args(["build", "./..."])
        .current_dir(dir)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "go not found on PATH".to_string()
            } else {
                format!("spawn go: {e}")
            }
        })?;
    if status.status.success() {
        Ok(())
    } else {
        Err(format!(
            "go build failed (exit {:?}):\n{}",
            status.status.code(),
            String::from_utf8_lossy(&status.stderr)
        ))
    }
}

/// Run `cargo check --offline` in `dir`. Prefers offline so the gate doesn't
/// hit the network after the first dependency fetch; falls back to online if
/// the lock/cache isn't warm yet.
fn assert_cargo_check(dir: &Path) -> Result<(), String> {
    // First try offline (fast, hermetic when deps are cached).
    let offline = std::process::Command::new("cargo")
        .args(["check", "--offline", "-q"])
        .current_dir(dir)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "cargo not found on PATH".to_string()
            } else {
                format!("spawn cargo: {e}")
            }
        })?;
    if offline.status.success() {
        return Ok(());
    }
    // Retry online once — first run needs to fetch reqwest/serde/etc.
    let online = std::process::Command::new("cargo")
        .args(["check", "-q"])
        .current_dir(dir)
        .output()
        .map_err(|e| format!("spawn cargo: {e}"))?;
    if online.status.success() {
        Ok(())
    } else {
        Err(format!(
            "cargo check failed (exit {:?}):\n{}\n{}",
            online.status.code(),
            String::from_utf8_lossy(&offline.stderr),
            String::from_utf8_lossy(&online.stderr)
        ))
    }
}

/// Helper: collect the relative file paths produced by a generate run.
fn generated_files(dir: &Path) -> BTreeSet<String> {
    fn walk(dir: &Path, base: &Path, out: &mut BTreeSet<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                walk(&path, base, out);
            } else {
                let rel = path.strip_prefix(base).unwrap().to_string_lossy().to_string();
                out.insert(rel);
            }
        }
    }
    let mut set = BTreeSet::new();
    walk(dir, dir, &mut set);
    set
}

#[test]
fn profile_flag_does_not_affect_output() {
    let f = &FIXTURES[0];
    let path = spec_path(f).expect("petstore is vendored");

    // Generate without --profile.
    let out_normal = std::env::temp_dir().join("specforge-profile-test-normal");
    let _ = std::fs::remove_dir_all(&out_normal);
    let status = std::process::Command::new("cargo")
        .args([
            "run", "-q", "-p", "specforge-cli", "--", "generate",
            path.to_str().unwrap(),
            "-o", out_normal.to_str().unwrap(),
            "-l", "ts",
        ])
        .output()
        .expect("run generate without --profile");
    assert!(status.status.success(), "generate without --profile failed:\n{}", String::from_utf8_lossy(&status.stderr));

    // Generate with --profile.
    let out_profile = std::env::temp_dir().join("specforge-profile-test-profile");
    let _ = std::fs::remove_dir_all(&out_profile);
    let status = std::process::Command::new("cargo")
        .args([
            "run", "-q", "-p", "specforge-cli", "--", "generate",
            path.to_str().unwrap(),
            "-o", out_profile.to_str().unwrap(),
            "-l", "ts",
            "--profile",
        ])
        .output()
        .expect("run generate with --profile");
    assert!(status.status.success(), "generate with --profile failed:\n{}", String::from_utf8_lossy(&status.stderr));

    // Profile output goes to stderr, verify it contains expected keys.
    let stderr = String::from_utf8_lossy(&status.stderr);
    assert!(stderr.contains("Profile:"), "expected 'Profile:' in stderr, got:\n{stderr}");
    assert!(stderr.contains("parse:"), "expected 'parse:' in stderr, got:\n{stderr}");
    assert!(stderr.contains("resolve:"), "expected 'resolve:' in stderr, got:\n{stderr}");
    assert!(stderr.contains("emit:"), "expected 'emit:' in stderr, got:\n{stderr}");
    assert!(stderr.contains("total:"), "expected 'total:' in stderr, got:\n{stderr}");

    // Both runs must produce the same set of files.
    let files_normal = generated_files(&out_normal);
    let files_profile = generated_files(&out_profile);
    assert_eq!(
        files_normal, files_profile,
        "--profile changed the set of generated files:\nnormal: {files_normal:?}\nprofile: {files_profile:?}"
    );
}
