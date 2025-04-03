use super::GenericParseConfig;
use crate::error::ChonkitError;

pub fn parse(input: &[u8], _config: &GenericParseConfig) -> Result<String, ChonkitError> {
    Ok(String::from_utf8_lossy(input).to_string())
}
