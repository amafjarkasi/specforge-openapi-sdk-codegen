#[test]
fn generate_and_check_docs() {
    let spec_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sample-api.yaml");
    let spec = specforge_core::parse_file(&spec_path).expect("parse");
    let doc = specforge_core::resolve(&spec).expect("resolve");
    let go_dir = tempfile::tempdir().unwrap();
    let opts = specforge_go::GeneratorOptions { out_dir: go_dir.path().to_path_buf(), module_path: None, package_name: None, i18n: None };
    let files = specforge_go::generate(&doc, &opts).expect("emit Go");
    assert!(!files.is_empty());
    let api_file = go_dir.path().join("api_pets.go");
    assert!(api_file.exists(), "api_pets.go not found");
    let content = std::fs::read_to_string(&api_file).unwrap();
    assert!(content.contains("// Returns"), "Missing return description");
    println!("=== Go doc comment verification PASSED ===");
    for line in content.lines().take(40) { println!("  {line}"); }
}
