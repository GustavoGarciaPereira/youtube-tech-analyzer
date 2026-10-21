use domain::Sentimento;
use domain::ports::AnalisadorDeSentimento;
use infra_nlp::download::{URL_MODELO, URL_TOKENIZER};
use infra_nlp::{ModelManager, RobertaAnalisador};

#[tokio::test]
#[ignore = "descarrega ~150 MB e corre inferência real; executar manualmente"]
async fn classifica_frases_positivas_e_negativas() {
    let pasta = tempfile::tempdir().expect("pasta temporária");
    let gestor = ModelManager::com_fontes(pasta.path().to_path_buf(), URL_MODELO, URL_TOKENIZER);
    let pasta_modelos = gestor.garantir_modelo_baixado().await.expect("download");

    let analisador = RobertaAnalisador::new(&pasta_modelos).expect("carregar modelo");
    assert_eq!(analisador.versao_modelo(), "roberta-sentiment-int8-v1");

    let positivo = analisador
        .analisar("This video is fantastic, thank you so much!")
        .await
        .expect("inferência positiva");
    let negativo = analisador
        .analisar("This is terrible and completely useless.")
        .await
        .expect("inferência negativa");

    assert_eq!(
        positivo.sentimento,
        Sentimento::Positivo,
        "score: {}",
        positivo.score
    );
    assert_eq!(
        negativo.sentimento,
        Sentimento::Negativo,
        "score: {}",
        negativo.score
    );
    assert!(positivo.score > 0.0);
    assert!(negativo.score < 0.0);
}
