//! *Composition root* da aplicação desktop.
//!
//! Este binário arranca a GUI `egui`, injeta as dependências de infraestrutura
//! e reutiliza os casos de uso da camada `application`.

#![forbid(unsafe_code)]

fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let caminho = desktop::wiring::caminho_db()?;
    let deps = desktop::wiring::montar(&caminho, None)?;
    let app_rt = rt.handle().clone();

    let opcoes = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 760.0])
            .with_title("YouTube Tech Analyzer"),
        ..Default::default()
    };

    eframe::run_native(
        "YouTube Tech Analyzer",
        opcoes,
        Box::new(move |cc| {
            Ok(Box::new(desktop::app::DesktopApp::new(
                cc, app_rt, deps, caminho,
            )))
        }),
    )
    .map_err(|erro| anyhow::anyhow!("falha ao iniciar a GUI: {erro}"))?;

    Ok(())
}
