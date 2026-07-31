//! Integration tests: parse + resolve the sample fixture and assert the IR.

use specforge_core::{
    parse_file, parse_str, resolve, HttpMethod, Model, ParamLocation, Scalar, Type,
};
use std::path::PathBuf;

fn sample() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("fixtures")
        .join("sample-api.yaml")
}

fn doc() -> specforge_core::Document {
    let spec = parse_file(sample()).expect("sample spec parses");
    resolve(&spec).expect("sample spec resolves")
}

#[test]
fn parses_top_level_metadata() {
    let d = doc();
    assert_eq!(d.title, "Sample API");
    assert_eq!(d.version, "1.0.0");
    assert_eq!(d.base_url.as_deref(), Some("https://api.example.com/v1"));
}

#[test]
fn resolves_bearer_security() {
    let d = doc();
    assert!(
        d.security
            .iter()
            .any(|s| matches!(s, specforge_core::SecurityScheme::HttpBearer)),
        "expected HttpBearer in {:?}",
        d.security
    );
}

#[test]
fn enum_models_render_as_enums() {
    let d = doc();
    let species = d.schemas.get("Species").expect("Species exists");
    let Model::Enum(e) = species else {
        panic!("Species should be an enum, got {species:?}");
    };
    assert_eq!(
        e.variants.iter().map(|v| &v.value).collect::<Vec<_>>(),
        &["dog", "cat", "bird", "reptile"]
    );
}

#[test]
fn object_model_has_properties_and_types() {
    let d = doc();
    let pet = d.schemas.get("Pet").expect("Pet exists");
    let Model::Object(o) = pet else {
        panic!("Pet should be an object");
    };

    let prop = |name: &str| {
        o.properties
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("missing property {name}"))
    };

    // uuid format → Uuid scalar
    assert!(matches!(prop("id").ty, Type::Scalar(Scalar::Uuid)));
    // date-time format → DateTime scalar
    assert!(matches!(
        prop("createdAt").ty,
        Type::Scalar(Scalar::DateTime)
    ));
    // $ref → Reference by name
    assert!(matches!(
        &prop("species").ty,
        Type::Reference { name, .. } if name == "Species"
    ));
    // array of $ref
    assert!(matches!(
        &prop("tags").ty,
        Type::Array { item, .. } if matches!(item.as_ref(), Type::Reference { name, .. } if name == "Tag")
    ));
    // nullable int stays Integer (nullable handled by emitter)
    assert!(matches!(prop("age").ty, Type::Scalar(Scalar::Integer)));

    // required set correctness
    assert!(prop("id").required);
    assert!(prop("name").required);
    assert!(!prop("age").required);
}

#[test]
fn composition_models_record_shape_type() {
    let d = doc();
    let event = d.schemas.get("PetEvent").expect("PetEvent exists");
    let Model::Object(o) = event else {
        panic!("PetEvent should be an object model with a shape_type");
    };
    assert!(o.properties.is_empty(), "oneOf root has no own props");
    let shape = o.shape_type.as_ref().expect("shape_type recorded");
    let Type::Composition(comp) = shape else {
        panic!("expected composition, got {shape:?}");
    };
    assert_eq!(comp.kind, specforge_core::CompositionKind::OneOf);
    assert_eq!(comp.members.len(), 3, "three oneOf members");
    // All members are references by name (not inlined).
    assert!(comp
        .members
        .iter()
        .all(|m| matches!(m, Type::Reference { .. })));
    // sample-api.yaml declares discriminator.propertyName: type
    assert_eq!(
        comp.discriminator
            .as_ref()
            .map(|d| d.property_name.as_str()),
        Some("type")
    );
}

#[test]
fn operations_are_resolved_with_params_and_bodies() {
    let d = doc();
    let by_id = |id: &str| d.operations.iter().find(|o| o.operation_id == id).unwrap();

    // GET /pets — query params
    let list = by_id("listPets");
    assert_eq!(list.method, HttpMethod::Get);
    assert_eq!(list.path, "/pets");
    let limit = list.parameters.iter().find(|p| p.name == "limit").unwrap();
    assert_eq!(limit.location, ParamLocation::Query);
    assert!(matches!(limit.ty, Type::Scalar(Scalar::Integer)));
    assert!(!limit.required);

    // POST /pets — request body
    let create = by_id("createPet");
    assert_eq!(create.method, HttpMethod::Post);
    let body = create.request_body.as_ref().expect("createPet has a body");
    assert!(body.required);
    assert!(matches!(&body.ty, Type::Reference { name, .. } if name == "NewPet"));

    // GET /pets/{petId} — path param
    let get = by_id("getPet");
    let petid = get.parameters.iter().find(|p| p.name == "petId").unwrap();
    assert_eq!(petid.location, ParamLocation::Path);
    assert!(petid.required);

    // Response bodies resolve to references.
    let created = create
        .responses
        .iter()
        .find(|r| r.status == "201")
        .expect("201 response");
    assert!(matches!(&created.body, Some(Type::Reference { name, .. }) if name == "Pet"));
}

#[test]
fn operations_carry_their_tag() {
    let d = doc();
    let tags: std::collections::HashSet<&str> = d
        .operations
        .iter()
        .filter_map(|o| o.tag.as_deref())
        .collect();
    assert!(tags.contains("Pets"));
    assert!(tags.contains("Store"));
}

#[test]
fn all_sample_operations_resolve_without_error() {
    // Smoke: every operation in the fixture must resolve to a non-empty id.
    let d = doc();
    assert!(d.operations.iter().all(|o| !o.operation_id.is_empty()));
    // Fixture defines: GET/POST /pets, GET/DELETE /pets/{petId}, POST /store/orders.
    assert_eq!(
        d.operations.len(),
        5,
        "expected exactly 5 operations, got {}",
        d.operations.len()
    );
}

#[test]
fn discriminator_mapping_survives_resolve() {
    let yaml = concat!(
        "openapi: \"3.0.0\"\n",
        "info:\n",
        "  title: Mapping Test\n",
        "  version: \"1.0.0\"\n",
        "paths: {}\n",
        "components:\n",
        "  schemas:\n",
        "    Dog:\n",
        "      type: object\n",
        "      properties:\n",
        "        breed:\n",
        "          type: string\n",
        "    Cat:\n",
        "      type: object\n",
        "      properties:\n",
        "        indoor:\n",
        "          type: boolean\n",
        "    Pet:\n",
        "      oneOf:\n",
        "        - \x24ref: \"#/components/schemas/Dog\"\n",
        "        - \x24ref: \"#/components/schemas/Cat\"\n",
        "      discriminator:\n",
        "        propertyName: petType\n",
        "        mapping:\n",
        "          dog: \"#/components/schemas/Dog\"\n",
        "          cat: \"#/components/schemas/Cat\"\n",
    );
    let spec = parse_str(yaml).expect("parses");
    let d = resolve(&spec).expect("resolves");
    let pet = d.schemas.get("Pet").expect("Pet exists");
    let Model::Object(o) = pet else {
        panic!("Pet should be object");
    };
    let shape = o.shape_type.as_ref().expect("shape_type");
    let Type::Composition(comp) = shape else {
        panic!("expected composition");
    };
    let disc = comp.discriminator.as_ref().expect("discriminator");
    assert_eq!(disc.property_name, "petType");
    let mapping = disc.mapping.as_ref().expect("mapping should be present");
    assert_eq!(mapping.get("dog").map(|s| s.as_str()), Some("Dog"));
    assert_eq!(mapping.get("cat").map(|s| s.as_str()), Some("Cat"));
}

// ─── Circular reference tests ───────────────────────────────────────────────

#[test]
fn self_referential_schema_resolves() {
    // A TreeNode with children: TreeNode[] should produce
    // Type::Array { item: Type::Reference { name: "TreeNode" } }
    let yaml = concat!(
        "openapi: \"3.0.0\"\n",
        "info:\n",
        "  title: Self Ref\n",
        "  version: \"1.0.0\"\n",
        "paths: {}\n",
        "components:\n",
        "  schemas:\n",
        "    TreeNode:\n",
        "      type: object\n",
        "      required: [id, label]\n",
        "      properties:\n",
        "        id:\n",
        "          type: string\n",
        "          format: uuid\n",
        "        label:\n",
        "          type: string\n",
        "        children:\n",
        "          type: array\n",
        "          items:\n",
        "            \x24ref: \"#/components/schemas/TreeNode\"\n",
    );
    let spec = parse_str(yaml).expect("parses");
    let d = resolve(&spec).expect("resolves");

    let node = d.schemas.get("TreeNode").expect("TreeNode exists");
    let Model::Object(o) = node else {
        panic!("TreeNode should be an object");
    };

    let prop = |name: &str| {
        o.properties
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("missing property {name}"))
    };

    // id is required uuid
    assert!(prop("id").required);
    assert!(matches!(prop("id").ty, Type::Scalar(Scalar::Uuid)));

    // label is required string
    assert!(prop("label").required);
    assert!(matches!(prop("label").ty, Type::Scalar(Scalar::String)));

    // children is optional array of self-reference
    let children = prop("children");
    assert!(!children.required);
    match &children.ty {
        Type::Array { item, .. } => match item.as_ref() {
            Type::Reference { name, .. } => assert_eq!(name, "TreeNode"),
            other => panic!("expected Reference, got {other:?}"),
        },
        other => panic!("expected Array, got {other:?}"),
    }
}

#[test]
fn mutual_references_resolve() {
    // A -> B, B -> A cycle: both resolve without panic,
    // properties reference each other by name.
    let yaml = concat!(
        "openapi: \"3.0.0\"\n",
        "info:\n",
        "  title: Mutual Ref\n",
        "  version: \"1.0.0\"\n",
        "paths: {}\n",
        "components:\n",
        "  schemas:\n",
        "    Alpha:\n",
        "      type: object\n",
        "      required: [name]\n",
        "      properties:\n",
        "        name:\n",
        "          type: string\n",
        "        beta:\n",
        "          \x24ref: \"#/components/schemas/Beta\"\n",
        "    Beta:\n",
        "      type: object\n",
        "      required: [value]\n",
        "      properties:\n",
        "        value:\n",
        "          type: integer\n",
        "        alpha:\n",
        "          \x24ref: \"#/components/schemas/Alpha\"\n",
    );
    let spec = parse_str(yaml).expect("parses");
    let d = resolve(&spec).expect("resolves");

    // Alpha resolves with a reference to Beta.
    let alpha = d.schemas.get("Alpha").expect("Alpha exists");
    let Model::Object(a) = alpha else {
        panic!("Alpha should be an object");
    };
    let beta_ref = a
        .properties
        .iter()
        .find(|p| p.name == "beta")
        .expect("Alpha has beta property");
    assert!(matches!(
        &beta_ref.ty,
        Type::Reference { name, .. } if name == "Beta"
    ));

    // Beta resolves with a reference to Alpha.
    let beta = d.schemas.get("Beta").expect("Beta exists");
    let Model::Object(b) = beta else {
        panic!("Beta should be an object");
    };
    let alpha_ref = b
        .properties
        .iter()
        .find(|p| p.name == "alpha")
        .expect("Beta has alpha property");
    assert!(matches!(
        &alpha_ref.ty,
        Type::Reference { name, .. } if name == "Alpha"
    ));
}

#[test]
fn allof_merges_properties_from_all_members() {
    let yaml = concat!(
        "openapi: \"3.0.0\"\n",
        "info:\n",
        "  title: AllOf Merge\n",
        "  version: \"1.0.0\"\n",
        "paths: {}\n",
        "components:\n",
        "  schemas:\n",
        "    Base:\n",
        "      type: object\n",
        "      required: [id]\n",
        "      properties:\n",
        "        id:\n",
        "          type: string\n",
        "          format: uuid\n",
        "    Timestamped:\n",
        "      type: object\n",
        "      properties:\n",
        "        createdAt:\n",
        "          type: string\n",
        "          format: date-time\n",
        "        updatedAt:\n",
        "          type: string\n",
        "          format: date-time\n",
        "    Extended:\n",
        "      allOf:\n",
        "        - \x24ref: \"#/components/schemas/Base\"\n",
        "        - \x24ref: \"#/components/schemas/Timestamped\"\n",
        "        - type: object\n",
        "          required: [name]\n",
        "          properties:\n",
        "            name:\n",
        "              type: string\n",
    );
    let spec = parse_str(yaml).expect("parses");
    let d = resolve(&spec).expect("resolves");

    let ext = d.schemas.get("Extended").expect("Extended exists");
    let Model::Object(o) = ext else {
        panic!("Extended should be an object");
    };

    let prop = |name: &str| {
        o.properties
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("missing property {name}"))
    };

    // Properties from all three members are present.
    assert!(matches!(prop("id").ty, Type::Scalar(Scalar::Uuid)));
    assert!(matches!(
        prop("createdAt").ty,
        Type::Scalar(Scalar::DateTime)
    ));
    assert!(matches!(
        prop("updatedAt").ty,
        Type::Scalar(Scalar::DateTime)
    ));
    assert!(matches!(prop("name").ty, Type::Scalar(Scalar::String)));

    // Required from Base (id) and inline (name) are unioned.
    assert!(prop("id").required);
    assert!(prop("name").required);
    assert!(!prop("createdAt").required);

    // shape_type is still the full AllOf composition.
    let shape = o.shape_type.as_ref().expect("shape_type recorded");
    let Type::Composition(comp) = shape else {
        panic!("expected composition, got {shape:?}");
    };
    assert_eq!(comp.kind, specforge_core::CompositionKind::AllOf);
    assert_eq!(comp.members.len(), 3);
}
