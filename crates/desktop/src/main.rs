//! *Composition root* da aplicação desktop.
//!
//! Este binário injeta as implementações concretas das portas nos casos de uso
//! e arranca a interface `egui`. Por agora apenas confirma que o workspace
//! compila e que o binário arranca.

#![forbid(unsafe_code)]

fn main() {
    let _ = dotenvy::dotenv();
    let metadados = application::MetadadosReproducao::new(infra_storage::VERSAO_SCHEMA, None);
    println!(
        "App a iniciar (commit {}, schema {})",
        metadados.hash_commit, metadados.versao_schema
    );
}
