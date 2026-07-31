#[test]
fn generate_and_check_docs() {
    let spec_path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sample-api.yaml");
    let spec = specforge_core::parse_file(&spec_path).expect("parse");
    let doc = specforge_core::resolve(&spec).expect("resolve");
    let ts_dir = tempfile::tempdir().unwrap();
    let opts = specforge_ts::GeneratorOptions {
        out_dir: ts_dir.path().to_path_buf(),
        package_name: None,
        i18n: None,
    };
    let files = specforge_ts::generate(&doc, &opts).expect("emit TS");
    assert!(!files.is_empty());
    let api_file = ts_dir.path().join("src").join("api").join("Pets.ts");
    assert!(api_file.exists(), "Pets.ts not found");
    let content = std::fs::read_to_string(&api_file).unwrap();
    assert!(content.contains("@param"), "Missing @param tags");
    assert!(content.contains("@returns"), "Missing @returns tag");
    assert!(content.contains("@throws"), "Missing @throws tag");
    println!("=== TS JSDoc verification PASSED ===");
    for line in content.lines().take(40) {
        println!("  {line}");
    }
}

#[test]
fn generate_with_i18n() {
    let spec_path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sample-api.yaml");
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
    assert!(
        content.contains("export const en"),
        "Missing English locale"
    );
    assert!(
        content.contains("export const es"),
        "Missing Spanish locale"
    );
    assert!(
        content.contains("Resource not found"),
        "Missing English error message"
    );
    assert!(
        content.contains("Recurso no encontrado"),
        "Missing Spanish error message"
    );
    assert!(
        content.contains("export function t("),
        "Missing translate helper"
    );
    assert!(
        content.contains("export type Locale"),
        "Missing Locale type"
    );
    println!("=== TS i18n verification PASSED ===");
    for line in content.lines().take(50) {
        println!("  {line}");
    }
}

#[test]
fn generate_version_file() {
    let spec_path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sample-api.yaml");
    let spec = specforge_core::parse_file(&spec_path).expect("parse");
    let doc = specforge_core::resolve(&spec).expect("resolve");
    let ts_dir = tempfile::tempdir().unwrap();
    let opts = specforge_ts::GeneratorOptions {
        out_dir: ts_dir.path().to_path_buf(),
        package_name: None,
        i18n: None,
    };
    let _files = specforge_ts::generate(&doc, &opts).expect("emit TS");

    // Verify specforge-version.json exists and has correct content.
    let version_file = ts_dir.path().join("specforge-version.json");
    assert!(version_file.exists(), "specforge-version.json not found");
    let content = std::fs::read_to_string(&version_file).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).expect("valid JSON");
    assert_eq!(v["ir_version"], specforge_core::IR_VERSION);
    assert!(
        v["specforge_version"].is_string(),
        "specforge_version should be a string"
    );
    assert!(
        v["spec_version"].is_string(),
        "spec_version should be a string"
    );
    assert!(
        v["generated_at"].is_string(),
        "generated_at should be a string"
    );
    println!("=== TS specforge-version.json verification PASSED ===");
    println!("Content:\n{content}");
}

/// Regression test: the generated TypeScript SDK must typecheck.
///
/// Template bugs in the emitter (e.g. unwired hooks, wrong-arity validator
/// helpers, broken imports) only surface when the generated `.ts` is compiled.
/// This test regenerates the SDK into an isolated temp dir, installs its
/// devDependencies (just `typescript`), and runs `tsc --noEmit`, so any future
/// emitter breakage fails CI.
#[test]
fn generated_sdk_typechecks() {
    if std::env::var_os("SKIP_COMPILE_TEST").is_some() {
        eprintln!("note: SKIP_COMPILE_TEST set; skipping generated_sdk_typechecks");
        return;
    }
    // Require node + npm + npx; skip gracefully if absent.
    for tool in ["node", "npm", "npx"] {
        let ok = std::process::Command::new(tool)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !ok {
            eprintln!("note: {tool} not available; skipping generated_sdk_typechecks");
            return;
        }
    }

    let spec_path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sample-api.yaml");
    let spec = specforge_core::parse_file(&spec_path).expect("parse");
    let doc = specforge_core::resolve(&spec).expect("resolve");

    let ts_dir = tempfile::tempdir().unwrap();
    let opts = specforge_ts::GeneratorOptions {
        out_dir: ts_dir.path().to_path_buf(),
        package_name: None,
        i18n: None,
    };
    let files = specforge_ts::generate(&doc, &opts).expect("emit TS");
    assert!(!files.is_empty(), "emitter produced no files");

    // Install devDependencies (typescript) without network noise.
    let install = std::process::Command::new("npm")
        .args(["install", "--no-audit", "--no-fund", "--loglevel=error"])
        .current_dir(ts_dir.path())
        .status()
        .expect("failed to spawn npm install");
    assert!(install.success(), "npm install of generated SDK failed");

    // Typecheck. `tsc --noEmit` returns non-zero on any type error.
    let status = std::process::Command::new("npx")
        .args(["tsc", "--noEmit"])
        .current_dir(ts_dir.path())
        .status()
        .expect("failed to spawn tsc");

    assert!(
        status.success(),
        "generated TypeScript SDK failed to typecheck (`tsc --noEmit` exited {status})"
    );
}
