use super::ParseConfig;
use crate::error::ChonkitError;

pub fn parse(input: &[u8], _config: &ParseConfig) -> Result<String, ChonkitError> {
    Ok(String::from_utf8_lossy(input).to_string())
}
