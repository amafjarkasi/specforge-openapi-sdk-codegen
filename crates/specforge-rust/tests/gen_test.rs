#[test]
fn generate_and_check_docs() {
    let spec_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sample-api.yaml");
    let spec = specforge_core::parse_file(&spec_path).expect("parse");
    let doc = specforge_core::resolve(&spec).expect("resolve");

    let rust_dir = tempfile::tempdir().unwrap();
    let opts = specforge_rust::GeneratorOptions {
        out_dir: rust_dir.path().to_path_buf(),
        crate_name: None,
    };
    let files = specforge_rust::generate(&doc, &opts).expect("emit Rust");
    assert!(!files.is_empty());

    let api_file = rust_dir.path().join("src").join("api").join("pets.rs");
    assert!(api_file.exists(), "pets.rs not found");
    let content = std::fs::read_to_string(&api_file).unwrap();

    assert!(content.contains("# Returns"), "Missing # Returns section");
    assert!(content.contains("# Errors"), "Missing # Errors section");

    println!("=== Rust doc comment verification passed ===");
    for line in content.lines().take(50) {
        println!("  {line}");
    }
}
