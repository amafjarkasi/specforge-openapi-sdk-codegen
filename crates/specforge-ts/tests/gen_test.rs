#[test]
fn generate_and_check_docs() {
    let spec_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sample-api.yaml");
    let spec = specforge_core::parse_file(&spec_path).expect("parse");
    let doc = specforge_core::resolve(&spec).expect("resolve");

    let ts_dir = tempfile::tempdir().unwrap();
    let opts = specforge_ts::GeneratorOptions {
        out_dir: ts_dir.path().to_path_buf(),
        package_name: None,
    };
    let files = specforge_ts::generate(&doc, &opts).expect("emit TS");
    assert!(!files.is_empty());

    let api_file = ts_dir.path().join("src").join("api").join("Pets.ts");
    assert!(api_file.exists(), "Pets.ts not found");
    let content = std::fs::read_to_string(&api_file).unwrap();

    assert!(content.contains("@param"), "Missing @param tags in TS operations");
    assert!(content.contains("@returns"), "Missing @returns tag in TS operations");
    assert!(content.contains("@throws"), "Missing @throws tag in TS operations");

    println!("=== TS JSDoc verification passed ===");
    for line in content.lines().take(40) {
        println!("  {line}");
    }
}
