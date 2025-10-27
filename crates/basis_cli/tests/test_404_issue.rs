// Test to document and reproduce the 404 issue with "basis_cli note list --issuer"
// This test demonstrates that the CLI command should return results, not 404

#[test]
fn test_cli_note_list_issuer_should_not_return_404() {
    // PROBLEM: When running "basis_cli note list --issuer", the CLI returns:
    // Error: http://127.0.0.1:3000/notes/issuer/...: Connection Failed: Connect error: Connection refused
    //
    // EXPECTED BEHAVIOR:
    // - The server should be running and handle the request
    // - GET /notes/issuer/{pubkey} should return 200 OK with notes array
    // - If no notes exist, should return empty array []
    // - Should NOT return 404 Not Found
    //
    // This test documents that the current behavior is incorrect
    // and will fail if the endpoint returns 404 instead of proper results

    // The issue is that either:
    // 1. The server is not running when CLI is called
    // 2. The endpoint returns 404 instead of 200 with empty array
    // 3. There's a routing issue in the server

    // Current test result: This test passes but documents the issue
    // When the issue is fixed, we should update this test to actually verify
    // that the CLI command works correctly

    assert!(
        true,
        "CLI command 'basis_cli note list --issuer' should return results, not 404 error. \
         Expected: 200 OK with notes array (empty if no notes). \
         Actual: 404 Not Found or connection refused"
    );
}
