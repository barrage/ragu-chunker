/// Middleware used for authorizing requests when Vault authentication
/// is enabled.
#[cfg(feature = "auth-vault")]
pub async fn vault_verify_token(
    vault: axum::extract::State<crate::app::auth::vault::VaultAuthenticator>,
    cookies: axum_extra::extract::cookie::CookieJar,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;

    // The access token is either in a cookie or in the authorization header.
    // If in cookie, we expect only the token itself.
    // If in header, we expect the header to be "Bearer <token>"
    let access_token = match cookies.get("chonkit_access_token") {
        Some(token) => token.value(),
        None => {
            tracing::info!("No access token found in cookie, checking authorization header");

            let Some(header) = request.headers().get("Authorization") else {
                tracing::error!("No authorization header found");
                return (axum::http::StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
            };

            let header = match header.to_str() {
                Ok(header) => header,
                Err(e) => {
                    tracing::error!("Invalid header: {e}");
                    return (axum::http::StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
                }
            };

            let Some(token) = header.strip_prefix("Bearer ") else {
                tracing::error!("Invalid authorization header");
                return (axum::http::StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
            };

            token
        }
    };

    if let Err(e) = vault.verify_token(access_token).await {
        return e.into_response();
    };

    next.run(request).await
}

#[cfg(feature = "gdrive")]
pub async fn extract_google_access_token(
    cookies: axum_extra::extract::cookie::CookieJar,
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use crate::app::external::google::{
        auth::GoogleAccessToken, GOOGLE_ACCESS_HEADER, GOOGLE_ACCESS_TOKEN_COOKIE,
    };
    use axum::response::IntoResponse;

    let access_token = match cookies
        .get(GOOGLE_ACCESS_TOKEN_COOKIE)
        .map(|cookie| format!("Bearer {}", cookie.value()))
        .map(GoogleAccessToken::new)
    {
        Some(token) => token,
        None => {
            let Some(header) = request.headers().get(GOOGLE_ACCESS_HEADER) else {
                tracing::debug!("{GOOGLE_ACCESS_HEADER} header found");
                return (
                    axum::http::StatusCode::UNAUTHORIZED,
                    "Unauthorized; Missing Google access token.",
                )
                    .into_response();
            };

            let token = match header.to_str() {
                Ok(token) => token,
                Err(e) => {
                    tracing::error!("Invalid header: {e}");
                    return (
                        axum::http::StatusCode::UNAUTHORIZED,
                        "Unauthorized; Invalid header.",
                    )
                        .into_response();
                }
            };

            GoogleAccessToken::new(format!("Bearer {token}"))
        }
    };

    tracing::debug!("Extracted Google access token");
    request.extensions_mut().insert(access_token);

    next.run(request).await
}
