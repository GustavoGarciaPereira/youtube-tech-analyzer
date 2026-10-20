//! *Composition root* da aplicação desktop.
//!
//! Este binário injeta as implementações concretas das portas nos casos de uso
//! e arranca a interface `egui`. Por agora apenas confirma que o workspace
//! compila e que o binário arranca.

#![forbid(unsafe_code)]

fn main() {
    println!("App a iniciar");
}
