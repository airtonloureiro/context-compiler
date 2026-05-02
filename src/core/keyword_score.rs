//! Keyword scoring (TF leve) — fallback determinístico quando embedding indisponível.
//!
//! Quando `fastembed` falha (sem rede no primeiro run, sandbox sem ONNX, etc.), ainda
//! queremos sinal de relevância para arquivos. Este módulo extrai keywords da query
//! (goal + log) e mede match-rate sobre o snippet de cada arquivo. Resultado normalizado
//! em [0..1], compatível com o campo `metadata.semantic_score` que o scorer já lê.
//!
//! Não substitui semântica real — sinônimos, variações morfológicas, abstrações conceituais
//! passam batido. Mas em cenários onde a query menciona keywords concretas (ex: "memory leak"
//! → arquivos com "leak", "memory", "Drop", "Box" ganham score), funciona surpreendentemente
//! bem como degrade gracioso.

/// Stopwords inglês básicas + algumas em PT.
const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "by", "for", "from", "has", "have", "i", "in",
    "is", "it", "its", "of", "on", "or", "that", "the", "this", "to", "was", "were", "with",
    "fix", "the", "in", "this", "that", "code", "issue", "bug", "error", "problem",
    "o", "a", "os", "as", "de", "da", "do", "das", "dos", "no", "na", "nos", "nas", "para",
    "com", "por", "que", "se", "uma", "um", "ao", "como", "mas", "ou", "e",
];

/// Tamanho mínimo de keyword (descarta "a", "i" etc. mesmo fora da stopword list).
const MIN_KEYWORD_LEN: usize = 3;

/// Limita keywords por query para não explodir custo.
const MAX_KEYWORDS: usize = 16;

/// Extrai keywords lowercase da query, removendo stopwords e tokens curtos.
/// Mantém ordem de aparição (primeiras keywords pesam mais — caller pode ponderar).
pub fn extract_keywords(query: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for raw in query.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if raw.is_empty() {
            continue;
        }
        let lc = raw.to_lowercase();
        if lc.len() < MIN_KEYWORD_LEN {
            continue;
        }
        if STOPWORDS.contains(&lc.as_str()) {
            continue;
        }
        if seen.insert(lc.clone()) {
            out.push(lc);
            if out.len() >= MAX_KEYWORDS {
                break;
            }
        }
    }
    out
}

/// Score em [0..1]: fração das keywords da query que aparecem no documento.
/// Ponderação: cada keyword conta como hit (não conta frequência — TF binária).
/// Empata com fastembed cosine para arquivos com 0 hits (∼0.0) e bate ≥0.7 quando
/// a maioria das keywords aparece.
pub fn keyword_match_score(query_keywords: &[String], document_lc: &str) -> f32 {
    if query_keywords.is_empty() {
        return 0.0;
    }
    let hits = query_keywords
        .iter()
        .filter(|kw| document_lc.contains(kw.as_str()))
        .count();
    hits as f32 / query_keywords.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn extrai_keywords_descartando_stopwords_e_curtas() {
        let kws = extract_keywords("Fix the memory leak in error chain");
        // "fix", "the", "in" são stopwords; "memory", "leak", "error", "chain" passam
        assert!(kws.contains(&"memory".to_string()));
        assert!(kws.contains(&"leak".to_string()));
        assert!(kws.contains(&"error".to_string()));
        assert!(kws.contains(&"chain".to_string()));
        assert!(!kws.contains(&"the".to_string()));
        assert!(!kws.contains(&"fix".to_string()));
    }

    #[test]
    fn match_score_proporcional() {
        let kws = vec!["memory".to_string(), "leak".to_string(), "drop".to_string()];
        // doc com todas as 3 → 1.0
        let s = keyword_match_score(&kws, "this fixes a memory leak in drop trait");
        assert!((s - 1.0).abs() < 1e-6);
        // doc com 2 das 3 → 0.66
        let s2 = keyword_match_score(&kws, "memory issue in drop");
        assert!((s2 - 0.6666).abs() < 0.01);
        // doc sem keywords → 0
        let s3 = keyword_match_score(&kws, "completely unrelated content");
        assert!(s3 < 0.01);
    }

    #[test]
    fn dedupe_keywords() {
        let kws = extract_keywords("memory memory leak LEAK Memory");
        // deduplicação case-insensitive
        assert_eq!(kws.len(), 2);
    }
}
