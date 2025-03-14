#[cfg(feature = "auth-jwt")]
#[derive(Clone)]
pub struct JwtVerifier {
    /// Verifier with JWK support. Verifies only the timestamps and signature.
    verifier: std::sync::Arc<jwtk::jwk::RemoteJwksVerifier>,
    required_iss: std::sync::Arc<str>,
}

#[cfg(feature = "auth-jwt")]
impl JwtVerifier {
    pub fn new(verifier: jwtk::jwk::RemoteJwksVerifier, required_iss: &str) -> Self {
        Self {
            verifier: std::sync::Arc::new(verifier),
            required_iss: required_iss.into(),
        }
    }

    pub async fn verify(&self, token: &str) -> bool {
        let token = match self
            .verifier
            .verify::<std::collections::HashMap<String, serde_json::Value>>(token)
            .await
        {
            Ok(token) => token,
            Err(e) => {
                tracing::error!("Failed to verify access token: {e}");
                return false;
            }
        };

        let claims = token.claims();

        let Some(ref iss) = claims.iss else {
            tracing::error!("Missing iss claim");
            return false;
        };

        if **iss != *self.required_iss {
            tracing::error!("Invalid iss claim");
            return false;
        }

        let Some(entitlements) = claims.extra.get("entitlements") else {
            tracing::error!("Missing entitlements claim");
            return false;
        };

        let Some(groups) = claims.extra.get("groups") else {
            tracing::error!("Missing groups claim");
            return false;
        };

        if !entitlements.as_array().is_some_and(|entitlements| {
            entitlements
                .iter()
                .any(|g| g.as_str().is_some_and(|g| g == "admin"))
        }) {
            tracing::error!("Missing admin app entitlement");
            return false;
        }

        if !groups.as_array().is_some_and(|groups| {
            groups
                .iter()
                .any(|g| g.as_str().is_some_and(|g| g == "ragu_admins"))
        }) {
            tracing::error!("Missing ragu_admins group");
            return false;
        }

        true
    }
}
