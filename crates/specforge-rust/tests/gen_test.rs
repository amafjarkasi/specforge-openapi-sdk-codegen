#[test]
fn generate_and_check_docs() {
    let spec_path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sample-api.yaml");
    let spec = specforge_core::parse_file(&spec_path).expect("parse");
    let doc = specforge_core::resolve(&spec).expect("resolve");
    let rust_dir = tempfile::tempdir().unwrap();
    let opts = specforge_rust::GeneratorOptions {
        out_dir: rust_dir.path().to_path_buf(),
        crate_name: None,
        i18n: None,
    };
    let files = specforge_rust::generate(&doc, &opts).expect("emit Rust");
    assert!(!files.is_empty());
    let api_file = rust_dir.path().join("src").join("api").join("pets.rs");
    assert!(api_file.exists(), "pets.rs not found");
    let content = std::fs::read_to_string(&api_file).unwrap();
    assert!(
        content.contains("# Returns") || content.contains("/// List"),
        "Missing doc comments"
    );
    println!("=== Rust doc comment verification PASSED ===");
    for line in content.lines().take(40) {
        println!("  {line}");
    }
}

/// Regression test: the generated Rust SDK must actually compile.
///
/// Earlier emitter changes (interceptors/validation_middleware modules,
/// ServiceContainer) shipped without ever compiling the output, so template
/// bugs (nested loops, missing struct fields, trait signature mismatches,
/// wrong arg order) silently broke every generated SDK. This test regenerates
/// the SDK into an isolated temp crate and runs `cargo check` on it, so any
/// future emitter template breakage fails CI.
#[test]
fn generated_sdk_compiles() {
    // Skip in environments without network/cargo (e.g. some CI sandboxes);
    // requires the rust toolchain and the ability to fetch deps.
    if std::env::var_os("SKIP_COMPILE_TEST").is_some() {
        eprintln!("note: SKIP_COMPILE_TEST set; skipping generated_sdk_compiles");
        return;
    }
    let cargo = std::process::Command::new("cargo")
        .arg("--version")
        .output();
    if !matches!(cargo, Ok(o) if o.status.success()) {
        eprintln!("note: cargo not available; skipping generated_sdk_compiles");
        return;
    }

    let spec_path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sample-api.yaml");
    let spec = specforge_core::parse_file(&spec_path).expect("parse");
    let doc = specforge_core::resolve(&spec).expect("resolve");

    let rust_dir = tempfile::tempdir().unwrap();
    let opts = specforge_rust::GeneratorOptions {
        out_dir: rust_dir.path().to_path_buf(),
        crate_name: None,
        i18n: None,
    };
    let files = specforge_rust::generate(&doc, &opts).expect("emit Rust");
    assert!(!files.is_empty(), "emitter produced no files");

    // The generated Cargo.toml is a leaf package; declare an empty workspace so
    // `cargo check` from this temp dir isn't absorbed into specforge's workspace.
    let cargo_toml = rust_dir.path().join("Cargo.toml");
    let mut manifest = std::fs::read_to_string(&cargo_toml).expect("read generated Cargo.toml");
    if !manifest.contains("[workspace]") {
        manifest.push_str("\n[workspace]\n");
    }
    std::fs::write(&cargo_toml, manifest).expect("write generated Cargo.toml");

    // `cargo check` the generated crate. Use the same toolchain, offline-friendly:
    // deps are fetched on demand; this is the real signal we want.
    let status = std::process::Command::new("cargo")
        .arg("check")
        .arg("--quiet")
        .current_dir(rust_dir.path())
        .env("CARGO_TARGET_DIR", rust_dir.path().join("target"))
        .status()
        .expect("failed to spawn cargo check");

    assert!(
        status.success(),
        "generated Rust SDK failed to compile (`cargo check` exited {status})"
    );
}

/// Deterministic output: generating the same spec twice must produce
/// byte-identical files. This is a core stability guarantee (STABILITY.md).
#[test]
fn output_is_deterministic() {
    let spec_path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sample-api.yaml");
    let spec = specforge_core::parse_file(&spec_path).expect("parse");
    let doc = specforge_core::resolve(&spec).expect("resolve");

    let opts = |dir: &std::path::Path| specforge_rust::GeneratorOptions {
        out_dir: dir.to_path_buf(),
        crate_name: None,
        i18n: None,
    };

    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let files_a = specforge_rust::generate(&doc, &opts(dir_a.path())).expect("emit A");
    let files_b = specforge_rust::generate(&doc, &opts(dir_b.path())).expect("emit B");

    // Same set of files produced.
    assert_eq!(files_a.len(), files_b.len());

    // Every file is byte-identical across both runs.
    for rel in &files_a {
        let path_a = dir_a.path().join(rel);
        let path_b = dir_b.path().join(rel);
        let content_a = std::fs::read(&path_a).unwrap_or_else(|e| panic!("read A/{rel}: {e}"));
        let content_b = std::fs::read(&path_b).unwrap_or_else(|e| panic!("read B/{rel}: {e}"));
        assert_eq!(
            content_a, content_b,
            "non-deterministic output in {rel}"
        );
    }
}
