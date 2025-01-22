use std::str::Utf8Error;

mod cursor;
pub mod semantic;
pub mod sliding;
pub mod snapping;
pub mod splitline;

pub use semantic::Semantic;
pub use sliding::SlidingWindow;
pub use snapping::Snapping;

#[derive(Debug, thiserror::Error)]
pub enum ChunkerError {
    #[error("{0}")]
    Config(String),

    #[error("utf-8: {0}")]
    Utf8(#[from] Utf8Error),
}
