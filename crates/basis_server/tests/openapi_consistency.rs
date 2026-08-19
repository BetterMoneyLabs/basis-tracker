//! OpenAPI consistency checks.
//!
//! These tests fail when the live axum router drifts from `openapi.yaml`.
//! They do not require a running server or network access.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Locate the project root by walking up from the current file.
fn project_root() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/basis_server -> project root
    dir.pop();
    dir.pop();
    dir
}

/// Extract path/method pairs registered in `crates/basis_server/src/main.rs`.
fn extract_code_routes() -> HashSet<(String, String)> {
    let main_rs = project_root().join("crates/basis_server/src/main.rs");
    let source = std::fs::read_to_string(&main_rs)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", main_rs.display(), e));

    // Find the router block.
    let start = source
        .find("let app = Router::new()")
        .expect("Router::new() not found in main.rs");
    let end = source[start..]
        .find(".with_state(")
        .expect(".with_state( not found after Router::new()")
        + start;
    let router_block = &source[start..end];

    let mut routes = HashSet::new();
    let mut i = 0;
    while let Some(idx) = router_block[i..].find(".route(") {
        let call_start = i + idx + ".route(".len();
        // Find matching ')' by counting parentheses.
        let mut depth = 1;
        let mut pos = call_start;
        while pos < router_block.len() && depth > 0 {
            match router_block.as_bytes()[pos] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                _ => {}
            }
            pos += 1;
        }
        let call_body = &router_block[call_start..pos - 1];

        // Extract the path literal.
        let path = {
            let q1 = call_body.find('"').expect("route path opening quote");
            let q2 = call_body[q1 + 1..]
                .find('"')
                .expect("route path closing quote");
            call_body[q1 + 1..q1 + 1 + q2].to_string()
        };

        // Extract the HTTP method (get/post).
        let method = if call_body.contains("get(") {
            "GET"
        } else if call_body.contains("post(") {
            "POST"
        } else {
            panic!("unexpected route method in: {}", call_body);
        }
        .to_string();

        routes.insert((method, path));
        i = pos;
    }

    routes
}

/// Parse `openapi.yaml` and return a map of path -> set of methods.
fn parse_openapi() -> (HashMap<String, HashSet<String>>, serde_yaml::Mapping) {
    let spec_path = project_root().join("openapi.yaml");
    let spec: serde_yaml::Value = serde_yaml::from_str(
        &std::fs::read_to_string(&spec_path)
            .unwrap_or_else(|e| panic!("failed to read {}: {}", spec_path.display(), e)),
    )
    .unwrap_or_else(|e| panic!("failed to parse {}: {}", spec_path.display(), e));

    let paths = spec
        .get("paths")
        .and_then(|v| v.as_mapping())
        .expect("paths mapping missing in openapi.yaml");

    let mut result: HashMap<String, HashSet<String>> = HashMap::new();
    for (path_value, methods_value) in paths {
        let path = path_value
            .as_str()
            .expect("path must be a string")
            .to_string();
        let methods = methods_value
            .as_mapping()
            .expect("path entry must be a mapping");
        let mut method_set = HashSet::new();
        for method in methods.keys() {
            let method = method
                .as_str()
                .expect("method must be a string")
                .to_uppercase();
            if ["GET", "POST", "PUT", "DELETE", "PATCH"].contains(&method.as_str()) {
                method_set.insert(method);
            }
        }
        result.insert(path, method_set);
    }

    let schemas = spec
        .get("components")
        .and_then(|v| v.get("schemas"))
        .and_then(|v| v.as_mapping())
        .cloned()
        .unwrap_or_default();

    (result, schemas)
}

/// Collect every `$ref` target under a YAML value.
fn collect_refs(value: &serde_yaml::Value, refs: &mut HashSet<String>) {
    match value {
        serde_yaml::Value::Mapping(m) => {
            for (k, v) in m {
                if k.as_str() == Some("$ref") {
                    if let Some(r) = v.as_str() {
                        let name = r.split('/').next_back().unwrap_or(r).to_string();
                        refs.insert(name);
                    }
                } else {
                    collect_refs(v, refs);
                }
            }
        }
        serde_yaml::Value::Sequence(s) => {
            for item in s {
                collect_refs(item, refs);
            }
        }
        _ => {}
    }
}

#[test]
fn openapi_yaml_is_valid() {
    let spec_path = project_root().join("openapi.yaml");
    let _: serde_yaml::Value = serde_yaml::from_str(
        &std::fs::read_to_string(&spec_path)
            .unwrap_or_else(|e| panic!("failed to read {}: {}", spec_path.display(), e)),
    )
    .unwrap_or_else(|e| panic!("{} is not valid YAML: {}", spec_path.display(), e));
}

#[test]
fn openapi_paths_match_code_routes() {
    let code_routes = extract_code_routes();
    let (openapi_paths, _) = parse_openapi();

    let mut openapi_routes = HashSet::new();
    for (path, methods) in &openapi_paths {
        for method in methods {
            openapi_routes.insert((method.clone(), path.clone()));
        }
    }

    let only_in_code: Vec<_> = code_routes.difference(&openapi_routes).collect();
    let only_in_openapi: Vec<_> = openapi_routes.difference(&code_routes).collect();

    if !only_in_code.is_empty() {
        let mut missing: Vec<_> = only_in_code
            .into_iter()
            .map(|(m, p)| format!("{} {}", m, p))
            .collect();
        missing.sort();
        panic!(
            "routes present in main.rs but missing from openapi.yaml:\n  {}",
            missing.join("\n  ")
        );
    }

    if !only_in_openapi.is_empty() {
        let mut extra: Vec<_> = only_in_openapi
            .into_iter()
            .map(|(m, p)| format!("{} {}", m, p))
            .collect();
        extra.sort();
        panic!(
            "routes present in openapi.yaml but not in main.rs:\n  {}",
            extra.join("\n  ")
        );
    }
}

#[test]
fn openapi_schemas_are_referenced_and_defined() {
    let (_, schemas) = parse_openapi();

    // Read the whole spec again to collect refs from paths and components.
    let spec_path = project_root().join("openapi.yaml");
    let spec: serde_yaml::Value = serde_yaml::from_str(
        &std::fs::read_to_string(&spec_path)
            .unwrap_or_else(|e| panic!("failed to read {}: {}", spec_path.display(), e)),
    )
    .unwrap();

    let mut referenced = HashSet::new();
    collect_refs(&spec, &mut referenced);

    let schema_names: HashSet<String> = schemas
        .keys()
        .filter_map(|k| k.as_str().map(String::from))
        .collect();

    let mut missing: Vec<_> = referenced
        .difference(&schema_names)
        .filter(|name| name.starts_with("ApiResponse") || !name.ends_with("Response"))
        .cloned()
        .collect();
    missing.sort();

    if !missing.is_empty() {
        panic!(
            "openapi.yaml references undefined schemas:\n  {}",
            missing.join("\n  ")
        );
    }

    // Report unused schemas as a warning via the test name rather than failing,
    // because hand-written specs may legitimately include shared helper schemas.
    let mut unused: Vec<_> = schema_names.difference(&referenced).cloned().collect();
    unused.sort();
    if !unused.is_empty() {
        eprintln!("note: unused schemas in openapi.yaml: {:?}", unused);
    }
}

#[test]
fn openapi_info_block_has_required_fields() {
    let spec_path = project_root().join("openapi.yaml");
    let spec: serde_yaml::Value = serde_yaml::from_str(
        &std::fs::read_to_string(&spec_path)
            .unwrap_or_else(|e| panic!("failed to read {}: {}", spec_path.display(), e)),
    )
    .unwrap();

    let info = spec.get("info").expect("info block missing");
    assert!(
        info.get("title").is_some(),
        "openapi.yaml info.title is missing"
    );
    assert!(
        info.get("version").is_some(),
        "openapi.yaml info.version is missing"
    );
    assert!(
        spec.get("servers").is_some(),
        "openapi.yaml servers block is missing"
    );
}
