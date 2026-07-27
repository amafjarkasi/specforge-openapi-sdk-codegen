#[test]
fn generate_and_check_docs() {
    let spec_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sample-api.yaml");
    let spec = specforge_core::parse_file(&spec_path).expect("parse");
    let doc = specforge_core::resolve(&spec).expect("resolve");
    let ts_dir = tempfile::tempdir().unwrap();
    let opts = specforge_ts::GeneratorOptions { out_dir: ts_dir.path().to_path_buf(), package_name: None, i18n: None };
    let files = specforge_ts::generate(&doc, &opts).expect("emit TS");
    assert!(!files.is_empty());
    let api_file = ts_dir.path().join("src").join("api").join("Pets.ts");
    assert!(api_file.exists(), "Pets.ts not found");
    let content = std::fs::read_to_string(&api_file).unwrap();
    assert!(content.contains("@param"), "Missing @param tags");
    assert!(content.contains("@returns"), "Missing @returns tag");
    assert!(content.contains("@throws"), "Missing @throws tag");
    println!("=== TS JSDoc verification PASSED ===");
    for line in content.lines().take(40) { println!("  {line}"); }
}

#[test]
fn generate_with_i18n() {
    let spec_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sample-api.yaml");
    let spec = specforge_core::parse_file(&spec_path).expect("parse");
    let doc = specforge_core::resolve(&spec).expect("resolve");
    let ts_dir = tempfile::tempdir().unwrap();
    let i18n = specforge_core::I18nConfig::from_locales(&["en".into(), "es".into()]);
    let opts = specforge_ts::GeneratorOptions {
        out_dir: ts_dir.path().to_path_buf(),
        package_name: None,
        i18n: Some(i18n),
    };
    let files = specforge_ts::generate(&doc, &opts).expect("emit TS with i18n");
    assert!(!files.is_empty());

    // Verify i18n.ts was generated.
    let i18n_file = ts_dir.path().join("src").join("i18n.ts");
    assert!(i18n_file.exists(), "i18n.ts not found");
    let content = std::fs::read_to_string(&i18n_file).unwrap();
    assert!(content.contains("export const en"), "Missing English locale");
    assert!(content.contains("export const es"), "Missing Spanish locale");
    assert!(content.contains("Resource not found"), "Missing English error message");
    assert!(content.contains("Recurso no encontrado"), "Missing Spanish error message");
    assert!(content.contains("export function t("), "Missing translate helper");
    assert!(content.contains("export type Locale"), "Missing Locale type");
    println!("=== TS i18n verification PASSED ===");
    for line in content.lines().take(50) { println!("  {line}"); }
}
