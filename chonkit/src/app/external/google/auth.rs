/// Used for capturing Google access tokens in request extensions.
#[derive(Debug, Clone)]
pub struct GoogleAccessToken(pub(super) String);

impl GoogleAccessToken {
    pub fn new(token: String) -> Self {
        Self(token)
    }
}
