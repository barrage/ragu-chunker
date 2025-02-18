use serde::Serialize;
use std::{ops::AddAssign, sync::Arc};

/// Holds tokenizers so we don't have to load them every time.
#[derive(Clone)]
pub struct Tokenizer {
    /// Tokenizer for ChatGPT and Ada models.
    cl100k: Arc<tiktoken_rs::CoreBPE>,

    /// Tokenizer for GPT-4 models.
    o200k: Arc<tiktoken_rs::CoreBPE>,
}

impl Tokenizer {
    pub fn new() -> Self {
        let chat_gpt_ada_tokenizer =
            tiktoken_rs::cl100k_base().expect("unable to load cl100k_base tokenizer");

        let gpt_4o_tokenizer =
            tiktoken_rs::o200k_base().expect("unable to load o200k_base tokenizer");

        Self {
            cl100k: Arc::new(chat_gpt_ada_tokenizer),
            o200k: Arc::new(gpt_4o_tokenizer),
        }
    }

    pub fn count(&self, text: &str) -> TokenCount {
        TokenCount::new(
            self.cl100k.encode_with_special_tokens(text).len(),
            self.o200k.encode_with_special_tokens(text).len(),
        )
    }
}

impl Default for Tokenizer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default, Clone, Copy, Serialize, utoipa::ToSchema)]
pub struct TokenCount {
    /// Number of tokens for ChatGPT and Ada models.
    pub cl100k: usize,

    /// Number of tokens for GPT-4o models.
    pub o200k: usize,
}

impl TokenCount {
    pub fn new(cl100k: usize, o200k: usize) -> Self {
        Self { cl100k, o200k }
    }
}

impl AddAssign for TokenCount {
    fn add_assign(&mut self, rhs: Self) {
        self.cl100k += rhs.cl100k;
        self.o200k += rhs.o200k;
    }
}
