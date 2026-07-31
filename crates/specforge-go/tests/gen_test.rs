#[test]
fn generate_and_check_docs() {
    let spec_path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sample-api.yaml");
    let spec = specforge_core::parse_file(&spec_path).expect("parse");
    let doc = specforge_core::resolve(&spec).expect("resolve");
    let go_dir = tempfile::tempdir().unwrap();
    let opts = specforge_go::GeneratorOptions {
        out_dir: go_dir.path().to_path_buf(),
        module_path: None,
        package_name: None,
        i18n: None,
    };
    let files = specforge_go::generate(&doc, &opts).expect("emit Go");
    assert!(!files.is_empty());
    let api_file = go_dir.path().join("api_pets.go");
    assert!(api_file.exists(), "api_pets.go not found");
    let content = std::fs::read_to_string(&api_file).unwrap();
    assert!(content.contains("// Returns"), "Missing return description");
    println!("=== Go doc comment verification PASSED ===");
    for line in content.lines().take(40) {
        println!("  {line}");
    }
}

/// Regression test: the generated Go SDK must compile.
///
/// Template bugs in the emitter only surface when the generated `.go` is built.
/// This test regenerates the SDK into an isolated temp dir and runs
/// `go build ./...`, so any future emitter breakage fails CI. It skips cleanly
/// when the Go toolchain is absent (e.g. in environments without Go installed);
/// CI matrices that include Go run it for real.
#[test]
fn generated_sdk_compiles() {
    if std::env::var_os("SKIP_COMPILE_TEST").is_some() {
        eprintln!("note: SKIP_COMPILE_TEST set; skipping generated_sdk_compiles");
        return;
    }
    // Resolve the Go binary. Prefer `go` on PATH, then the standard install
    // location (matching scripts/generate-examples.sh, which adds
    // /usr/local/go/bin). Skip gracefully if neither is runnable.
    let go = ["go", "/usr/local/go/bin/go"].iter().find_map(|candidate| {
        let ok = std::process::Command::new(candidate)
            .arg("version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok {
            Some(*candidate)
        } else {
            None
        }
    });
    let go = match go {
        Some(g) => g,
        None => {
            eprintln!("note: go toolchain not available; skipping generated_sdk_compiles");
            return;
        }
    };

    let spec_path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sample-api.yaml");
    let spec = specforge_core::parse_file(&spec_path).expect("parse");
    let doc = specforge_core::resolve(&spec).expect("resolve");
    let go_dir = tempfile::tempdir().unwrap();
    let opts = specforge_go::GeneratorOptions {
        out_dir: go_dir.path().to_path_buf(),
        module_path: Some("example.com/generated".to_string()),
        package_name: None,
        i18n: None,
    };
    let files = specforge_go::generate(&doc, &opts).expect("emit Go");
    assert!(!files.is_empty(), "emitter produced no files");

    // The generated SDK uses only the standard library, so this builds offline.
    let status = std::process::Command::new(go)
        .args(["build", "./..."])
        .current_dir(go_dir.path())
        .status()
        .expect("failed to spawn go build");

    assert!(
        status.success(),
        "generated Go SDK failed to compile (`go build ./...` exited {status})"
    );
}
