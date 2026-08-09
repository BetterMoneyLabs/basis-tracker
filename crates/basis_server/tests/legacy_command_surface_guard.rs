#[test]
fn production_actor_has_no_legacy_proof_commands_or_repair_environment() {
    let library = include_str!("../src/lib.rs");
    let binary = include_str!("../src/main.rs");
    let library_code = library
        .lines()
        .filter(|line| !line.trim_start().starts_with("//!"))
        .collect::<Vec<_>>()
        .join("\n");
    let forbidden_commands = [
        "GenerateProof",
        "GetTrackerLookupProof",
        "GetReserveLookupProof",
        "GetReserveInsertProof",
        "GetReserveStateDigest",
    ];

    for command in forbidden_commands {
        assert!(
            !library_code.contains(command),
            "legacy TrackerCommand variant remains: {command}"
        );
        assert!(
            !binary.contains(command),
            "legacy tracker actor arm remains: {command}"
        );
    }

    assert!(!binary.contains("REPAIR_RESERVE_"));
}

#[test]
fn openapi_exposes_exactly_the_nine_retired_routes_as_gone() {
    let openapi = include_str!("../../../openapi.yaml");
    assert!(!openapi.contains("  /proof:\n"));

    let routes = [
        "/redeem:",
        "/redeem/complete:",
        "/proof/redemption:",
        "/tracker/proof:",
        "/reserve/proof:",
        "/tracker/signature:",
        "/redemption/prepare:",
        "/redemption/build:",
        "/redemption/submit:",
    ];

    for (index, route) in routes.iter().enumerate() {
        let start = openapi
            .find(&format!("  {route}\n"))
            .unwrap_or_else(|| panic!("missing retired OpenAPI route {route}"));
        let end = routes
            .iter()
            .filter_map(|other| openapi[start + 1..].find(&format!("  {other}\n")))
            .min()
            .map(|offset| start + 1 + offset)
            .unwrap_or(openapi.len());
        let block = &openapi[start..end];
        assert!(
            block.contains("deprecated: true"),
            "{route} is not deprecated"
        );
        assert!(
            block.contains("'410':"),
            "{route} does not document HTTP 410"
        );
        assert!(
            !block.contains("'200':"),
            "{route} still documents a successful response"
        );
        assert_eq!(
            openapi.matches(&format!("  {route}\n")).count(),
            1,
            "{route} appears more than once (index {index})"
        );
    }
}
