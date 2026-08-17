//! Role-based authorization middleware.
//!
//! Reads the [`AuthContext`] produced by [`crate::auth_middleware`] and the
//! [`axum::extract::MatchedPath`] extension, then rejects requests whose role
//! is insufficient for the matched route.

use axum::{
    extract::{MatchedPath, Request},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::auth_middleware::AuthContext;
use crate::config::ClientRole;

/// Middleware entry point. Run this *after* [`crate::auth_middleware`] so that
/// an [`AuthContext`] extension is guaranteed to exist.
pub async fn authorization_middleware(request: Request, next: Next) -> Response {
    let path = request
        .extensions()
        .get::<MatchedPath>()
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| request.uri().path().to_string());
    let method = request.method().as_str().to_uppercase();

    let required = required_role(&path, &method);

    let ctx = request
        .extensions()
        .get::<AuthContext>()
        .cloned()
        .unwrap_or(AuthContext {
            pubkey: None,
            role: ClientRole::Read,
            auth_mode: crate::config::AuthMode::None,
        });

    if role_sufficient(ctx.role, required) {
        next.run(request).await
    } else {
        forbidden(format!(
            "Insufficient privileges: route requires {:?} access",
            required
        ))
    }
}

/// Maps a matched route pattern and HTTP method to a required role.
fn required_role(path: &str, method: &str) -> ClientRole {
    // Health check is handled as public by the auth middleware, but keep it Read here.
    if path == "/" && method == "GET" {
        return ClientRole::Read;
    }

    match method {
        "POST" => match path {
            // Admin-only: reserve creation submission and policy management.
            "/reserves/create" | "/reserves/submit" | "/acceptance/policy" => ClientRole::Admin,
            // Write: state-changing redemption and note operations.
            "/notes"
            | "/redeem"
            | "/redeem/complete"
            | "/redemption/prepare"
            | "/redemption/build"
            | "/redemption/submit"
            | "/tracker/signature" => ClientRole::Write,
            // Read-like POSTs: querying state without mutation.
            "/notes/state" | "/acceptance/check" => ClientRole::Read,
            // Unknown POSTs default to Admin for safety.
            _ => ClientRole::Admin,
        },
        // All GET endpoints expose tracker state and are readable.
        "GET" => ClientRole::Read,
        // OPTIONS preflight is handled at the CORS layer before auth/authorization.
        "OPTIONS" => ClientRole::Read,
        // Any other verb is unexpected; default to Admin.
        _ => ClientRole::Admin,
    }
}

fn role_sufficient(have: ClientRole, need: ClientRole) -> bool {
    use ClientRole::*;
    matches!(
        (have, need),
        (Admin, _) | (Write, Write) | (Write, Read) | (Read, Read)
    )
}

fn forbidden(message: impl AsRef<str>) -> Response {
    (
        StatusCode::FORBIDDEN,
        [(
            axum::http::header::CONTENT_TYPE,
            "application/json; charset=utf-8",
        )],
        format!(
            r#"{{"success":false,"data":null,"error":"{}"}}"#,
            message.as_ref().replace('"', "\\\"")
        ),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_ordering() {
        assert!(role_sufficient(ClientRole::Admin, ClientRole::Admin));
        assert!(role_sufficient(ClientRole::Admin, ClientRole::Write));
        assert!(role_sufficient(ClientRole::Admin, ClientRole::Read));
        assert!(role_sufficient(ClientRole::Write, ClientRole::Write));
        assert!(role_sufficient(ClientRole::Write, ClientRole::Read));
        assert!(role_sufficient(ClientRole::Read, ClientRole::Read));
        assert!(!role_sufficient(ClientRole::Read, ClientRole::Write));
        assert!(!role_sufficient(ClientRole::Read, ClientRole::Admin));
        assert!(!role_sufficient(ClientRole::Write, ClientRole::Admin));
    }

    #[test]
    fn required_roles_match_expectations() {
        assert_eq!(required_role("/", "GET"), ClientRole::Read);
        assert_eq!(required_role("/notes", "GET"), ClientRole::Read);
        assert_eq!(required_role("/notes", "POST"), ClientRole::Write);
        assert_eq!(required_role("/redeem", "POST"), ClientRole::Write);
        assert_eq!(
            required_role("/redemption/build", "POST"),
            ClientRole::Write
        );
        assert_eq!(
            required_role("/acceptance/policy", "POST"),
            ClientRole::Admin
        );
        assert_eq!(required_role("/notes/state", "POST"), ClientRole::Read);
        assert_eq!(required_role("/acceptance/check", "POST"), ClientRole::Read);
    }
}
