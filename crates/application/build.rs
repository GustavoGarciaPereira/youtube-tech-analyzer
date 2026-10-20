use std::process::Command;

fn main() {
    let hash = Command::new("git")
        .args(["rev-parse", "--short=8", "HEAD"])
        .output()
        .ok()
        .and_then(|saida| {
            if saida.status.success() {
                String::from_utf8(saida.stdout).ok()
            } else {
                None
            }
        })
        .map(|hash| hash.trim().to_owned())
        .unwrap_or_else(|| "desconhecido".to_owned());

    println!("cargo:rustc-env=GIT_HASH={hash}");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
}
