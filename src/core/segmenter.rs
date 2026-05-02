use super::context_item::{ContextItem, ContextItemType};

pub struct Segmenter;

impl Segmenter {
    /// Quebra itens longos em segmentos menores (Sentence-level / Paragraph-level chunking).
    /// Baseado em pesquisas como CPC (Context-aware Prompt Compression), que prova que 
    /// a poda em nível de sentença/parágrafo resulta em melhor retenção de fatos do que cortar o arquivo inteiro.
    pub fn segment(items: Vec<ContextItem>) -> Vec<ContextItem> {
        let mut segmented = Vec::new();
        
        for item in items {
            // Documentos ou Logs gigantes são os maiores vilões do contexto sujo.
            // Códigos são tratados pela AST Skeletonization no Adapter, então não quebramos Code.
            let is_target = matches!(
                item.item_type, 
                ContextItemType::Document | ContextItemType::Log | ContextItemType::Message
            );

            // Só fragmenta itens com mais de ~1000 caracteres
            if is_target && item.content.len() > 1000 {
                // Fragmenta pelo delimitador duplo de nova linha (parágrafos) ou simples dependendo da densidade
                let chunks: Vec<&str> = if item.content.contains("\n\n") {
                    item.content.split("\n\n").collect()
                } else {
                    item.content.split('\n').collect()
                };

                for (idx, chunk) in chunks.into_iter().enumerate() {
                    let text = chunk.trim();
                    if text.is_empty() { continue; }

                    let mut new_item = item.clone();
                    new_item.id = format!("{}_seg{}", item.id, idx);
                    new_item.content = text.to_string();
                    
                    // Adicionar indicador de segmento na source para o Evidence Pointers
                    if let Some(serde_json::Value::Object(ref mut map)) = new_item.source {
                        map.insert("segment_index".to_string(), serde_json::json!(idx));
                    }
                    
                    segmented.push(new_item);
                }
            } else {
                // Mantém íntegro se for Code ou menor que o limite.
                segmented.push(item);
            }
        }
        
        segmented
    }
}
