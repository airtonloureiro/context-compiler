use super::symbol_map::Symbol;

pub struct ContextSlicer;

impl ContextSlicer {
    /// Fatiamento (slicing) básico do código.
    /// Retorna apenas o trecho de código referente ao símbolo fatiado.
    pub fn slice(content: &str, symbol: &Symbol) -> String {
        // Se a parsing error ou não tivermos os bytes corretos,
        // retornamos vazio e lidamos com isso no chamador.
        if symbol.range.start_byte >= content.len() || symbol.range.end_byte > content.len() {
            return String::new();
        }
        
        content[symbol.range.start_byte..symbol.range.end_byte].to_string()
    }
    
    /// Fatia o código extraindo apenas a assinatura (útil para economizar tokens 
    /// mantendo apenas a interface pública).
    pub fn slice_signature(content: &str, symbol: &Symbol) -> String {
        if symbol.signature_range.start_byte >= content.len() || symbol.signature_range.end_byte > content.len() {
            return String::new();
        }
        
        content[symbol.signature_range.start_byte..symbol.signature_range.end_byte].to_string()
    }
}
