# Rqbit Authentication Research

**Date:** February 15, 2026  
**Task:** API-002.1 - Research rqbit auth methods  
**Researcher:** AI Agent

## Executive Summary

rqbit supports **HTTP Basic Authentication** as its primary authentication mechanism for the HTTP API. This is a simple, single-user authentication scheme suitable for protecting the API from unauthorized access.

## Authentication Method

### HTTP Basic Authentication

rqbit implements standard HTTP Basic Authentication ([RFC 7617](https://tools.ietf.org/html/rfc7617)) using the `Authorization` header.

#### Configuration

Authentication is configured via the environment variable:

```bash
RQBIT_HTTP_BASIC_AUTH_USERPASS=username:password rqbit server start ...
```

**Format:** Single user credential in the format `username:password`

#### Implementation Details

**Source File:** `rqbit/crates/librqbit/src/http_api/mod.rs` (lines 44-76)

The authentication flow:

1. **Enable Auth:** Set via `HttpApiOptions.basic_auth: Option<(String, String)>`
2. **Middleware:** Applied as axum middleware layer when auth is configured
3. **Header Parsing:** Extracts `Authorization: Basic <base64>` header
4. **Validation:** Decodes base64 and validates username:password against configured credentials
5. **Response:** Returns `401 Unauthorized` with `WWW-Authenticate: Basic realm="API"` on failure

```rust
// Key implementation points from http_api/mod.rs:
pub struct HttpApiOptions {
    pub read_only: bool,
    pub basic_auth: Option<(String, String)>,  // (username, password)
    pub allow_create: bool,
    // ...
}

async fn simple_basic_auth(
    expected_username: Option<&str>,
    expected_password: Option<&str>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Result<axum::response::Response> {
    // Extract Authorization header
    let user_pass = headers
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Basic "))
        .and_then(|v| base64::engine::general_purpose::STANDARD.decode(v).ok())
        .and_then(|v| String::from_utf8(v).ok());
    
    // Return 401 if no auth header
    if user_pass.is_none() {
        return Ok((
            StatusCode::UNAUTHORIZED,
            [("WWW-Authenticate", "Basic realm=\"API\"")],
        ).into_response());
    }
    
    // Validate credentials
    match user_pass.split_once(':') {
        Some((u, p)) if u == expected_user && p == expected_pass => {
            Ok(next.run(request).await)
        }
        _ => Err(ApiError::unauthorized()),
    }
}
```

#### Security Considerations

1. **Single User Only:** Only supports one username/password pair
2. **No Constant-Time Compare:** The TODO comment (line 71) indicates timing attack vulnerability:
   ```rust
   // TODO: constant time compare
   ```
3. **Plaintext Credentials:** Credentials stored in memory as plaintext strings
4. **Base64 Encoding:** Not encryption - credentials are merely base64-encoded in transit
5. **HTTPS Recommended:** Basic auth should only be used over HTTPS in production

#### Error Responses

- **401 Unauthorized:** Missing or invalid credentials
  - Header: `WWW-Authenticate: Basic realm="API"`
- **403 Forbidden:** Valid credentials but insufficient permissions (not currently used)

## Implications for torrent-fuse

### Client-Side Implementation Required

To support authenticated rqbit servers, torrent-fuse needs:

1. **Credential Configuration:**
   - Add `username` and `password` fields to client config
   - Support environment variable (e.g., `RQBIT_AUTH_USERPASS`)

2. **Request Header Injection:**
   - Base64-encode credentials: `base64(username:password)`
   - Add header to all HTTP requests: `Authorization: Basic <encoded>`

3. **Error Handling:**
   - Handle 401 responses gracefully
   - Parse `WWW-Authenticate` header for auth requirements
   - Provide clear error messages for auth failures

### Example Implementation

```rust
use base64::Engine;

fn create_auth_header(username: &str, password: &str) -> String {
    let credentials = format!("{}:{}", username, password);
    let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
    format!("Basic {}", encoded)
}

// In request builder:
let client = reqwest::Client::new();
let response = client
    .get("http://localhost:3030/torrents")
    .header("Authorization", create_auth_header(&username, &password))
    .send()
    .await?;
```

## Alternative Authentication Methods

As of the current version (v9.0.0-beta.2), rqbit does **NOT** support:

- API tokens/keys
- OAuth/OIDC
- Client certificates (mTLS)
- JWT tokens
- Multi-user authentication

The only authentication mechanism available is HTTP Basic Auth.

## References

1. **rqbit GitHub:** https://github.com/ikatson/rqbit
2. **Basic Auth Section:** README.md "Basic auth" section
3. **Implementation:** `rqbit/crates/librqbit/src/http_api/mod.rs` (lines 34-42, 44-76, 151-163)
4. **RFC 7617:** The 'Basic' HTTP Authentication Scheme

## Recommendations

1. **Implement Basic Auth Support:** Add username/password configuration to torrent-fuse
2. **Security Warning:** Document that Basic Auth credentials are sent with every request
3. **HTTPS Enforcement:** Recommend/require HTTPS for production deployments
4. **Future-Proofing:** Design auth configuration to allow for token-based auth if rqbit adds support
