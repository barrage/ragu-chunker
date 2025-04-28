use super::StringParseConfig;
use crate::error::ChonkitError;

pub(super) fn parse(_config: &StringParseConfig, input: &[u8]) -> Result<String, ChonkitError> {
    Ok(String::from_utf8_lossy(input).to_string())
}
