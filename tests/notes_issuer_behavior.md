# Notes/Issuer Endpoint Behavior Verification

## Expected Behavior

When the `/notes/issuer/{pubkey}` endpoint is called and no notes exist for the given issuer:

- **HTTP Status**: `200 OK` (not 404)
- **Response Body**:
  ```json
  {
    "success": true,
    "data": [],
    "error": null
  }
  ```

## Implementation Verification

Based on the code analysis:

1. **Endpoint Handler**: `crates/basis_server/src/api.rs` lines 253-276
   - When notes are successfully retrieved (even if empty), returns `StatusCode::OK`
   - Uses `success_response(serializable_notes)` helper
   - Empty `Vec<IouNote>` becomes `data: Some([])` in JSON response

2. **Demo Script Usage**: `demo/bob_receiver.sh` lines 31-50
   - Expects HTTP 200 response with empty array when no notes exist
   - Polls endpoint and processes notes only when `data` array has length > 0

3. **Error Handling**: `crates/basis_server/src/api.rs` lines 202-225
   - Invalid hex encoding: returns `400 Bad Request`
   - Wrong byte length: returns `400 Bad Request`

## Test Coverage

The current implementation correctly returns an empty list instead of 404 when no notes exist for an issuer.