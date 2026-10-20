use std::fmt::Write as _;

use sha2::{Digest, Sha256};

/// Hash SHA-256 hex do conteúdo do vídeo e do template do prompt.
///
/// Formato: `titulo + "\n" + descricao + "\n" + tags.join(",") + "\n" + template`.
#[must_use]
pub fn hash_conteudo(titulo: &str, descricao: &str, tags: &[String], template: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(titulo.as_bytes());
    hasher.update(b"\n");
    hasher.update(descricao.as_bytes());
    hasher.update(b"\n");
    hasher.update(tags.join(",").as_bytes());
    hasher.update(b"\n");
    hasher.update(template.as_bytes());

    let mut hex = String::with_capacity(64);
    for byte in hasher.finalize() {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::hash_conteudo;

    fn tags() -> Vec<String> {
        vec!["rust".to_owned(), "programação".to_owned()]
    }

    #[test]
    fn mesma_entrada_produz_mesmo_hash() {
        let primeiro = hash_conteudo("Título", "Descrição", &tags(), "template");
        let segundo = hash_conteudo("Título", "Descrição", &tags(), "template");
        assert_eq!(primeiro, segundo);
        assert_eq!(primeiro.len(), 64);
    }

    #[test]
    fn mudar_titulo_muda_hash() {
        let base = hash_conteudo("Título", "Descrição", &tags(), "template");
        let outro = hash_conteudo("Outro título", "Descrição", &tags(), "template");
        assert_ne!(base, outro);
    }

    #[test]
    fn mudar_descricao_muda_hash() {
        let base = hash_conteudo("Título", "Descrição", &tags(), "template");
        let outro = hash_conteudo("Título", "Outra descrição", &tags(), "template");
        assert_ne!(base, outro);
    }

    #[test]
    fn mudar_tags_ou_template_muda_hash() {
        let base = hash_conteudo("Título", "Descrição", &tags(), "template");

        let mut outras_tags = tags();
        outras_tags.push("tokio".to_owned());
        assert_ne!(
            base,
            hash_conteudo("Título", "Descrição", &outras_tags, "template")
        );
        assert_ne!(
            base,
            hash_conteudo("Título", "Descrição", &tags(), "outro template")
        );
    }
}
