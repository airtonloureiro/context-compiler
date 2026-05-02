use tiktoken_rs::{cl100k_base, CoreBPE};
use std::sync::OnceLock;

static TOKENIZER: OnceLock<CoreBPE> = OnceLock::new();

pub struct Tokenizer;

impl Tokenizer {
    /// Retorna a instância do tokenizer (singleton).
    pub fn get() -> &'static CoreBPE {
        TOKENIZER.get_or_init(|| {
            // Usa o cl100k_base (padrão do GPT-4, OpenAI)
            cl100k_base().unwrap()
        })
    }

    /// Calcula a quantidade exata de tokens de um texto usando o `cl100k_base`.
    pub fn count_tokens(text: &str) -> usize {
        let bpe = Self::get();
        bpe.encode_with_special_tokens(text).len()
    }
}
