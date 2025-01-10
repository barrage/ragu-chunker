#[cfg(feature = "auth-vault")]
pub mod vault;

#[cfg(feature = "auth-vault")]
#[derive(Debug, serde::Deserialize)]
struct ChonkitJwt {
    aud: String,
    exp: i64,
    version: usize,
}
