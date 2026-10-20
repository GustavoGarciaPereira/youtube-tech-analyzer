use serde::Serialize;

use crate::errors::AnalysisError;
use crate::mann_kendall::variancia_s;

const Z_95: f64 = 1.959_963_984_540_054;

/// Resultado da regressão robusta de Theil-Sen.
#[derive(Debug, Clone, Serialize)]
pub struct ResultadoTheilSen {
    /// Declive mediano dos pares, em unidades por passo.
    pub slope: f64,
    /// Intercepto mediano.
    pub intercept: f64,
    /// Intervalo de confiança de 95% para o declive.
    pub slope_ic95: (f64, f64),
    /// Número de observações.
    pub n: usize,
}

/// Regressão robusta de Theil-Sen.
///
/// O declive é a mediana de `(y_j - y_i) / (j - i)` para todos os pares
/// `i < j`. O intercepto é a mediana de `y_i - slope * i`.
///
/// O IC95 usa a aproximação analítica de Sen (1968), determinística:
///
/// `C = z_0.975 * sqrt(Var(S))`, com `N = n(n-1)/2` pares,
/// `L = floor((N - C)/2)` e `U = ceil((N + C)/2)`.
/// Os limites são o L-ésimo e o U-ésimo declives ordenados (1-based),
/// truncados ao intervalo `[1, N]`. `Var(S)` é a mesma variância com
/// correção para empates usada pelo Mann-Kendall.
pub fn theil_sen(serie: &[f64]) -> Result<ResultadoTheilSen, AnalysisError> {
    let n = serie.len();
    if n < 3 {
        return Err(AnalysisError::SerieCurta { n });
    }

    let mut declives = Vec::with_capacity(n * (n - 1) / 2);
    for i in 0..n {
        for j in (i + 1)..n {
            declives.push((serie[j] - serie[i]) / (j - i) as f64);
        }
    }

    let slope = mediana(&mut declives).ok_or(AnalysisError::SerieCurta { n })?;
    let mut interceptos = Vec::with_capacity(n);
    for (i, y) in serie.iter().enumerate() {
        interceptos.push(y - slope * i as f64);
    }
    let intercept = mediana(&mut interceptos).ok_or(AnalysisError::SerieCurta { n })?;

    let n_pares = declives.len();
    let c = Z_95 * variancia_s(serie).sqrt();
    let inferior =
        (((n_pares as f64 - c) / 2.0).floor() as isize).clamp(1, n_pares as isize) as usize;
    let superior =
        (((n_pares as f64 + c) / 2.0).ceil() as isize).clamp(1, n_pares as isize) as usize;
    let slope_ic95 = (declives[inferior - 1], declives[superior - 1]);

    Ok(ResultadoTheilSen {
        slope,
        intercept,
        slope_ic95,
        n,
    })
}

fn mediana(valores: &mut [f64]) -> Option<f64> {
    if valores.is_empty() {
        return None;
    }
    valores.sort_by(f64::total_cmp);
    let meio = valores.len() / 2;
    if valores.len() % 2 == 1 {
        valores.get(meio).copied()
    } else {
        let esquerda = valores.get(meio - 1).copied()?;
        let direita = valores.get(meio).copied()?;
        Some((esquerda + direita) / 2.0)
    }
}

#[cfg(test)]
mod tests {
    use super::theil_sen;
    use crate::errors::AnalysisError;

    fn crescente(inicio: i32, fim: i32) -> Vec<f64> {
        (inicio..=fim).map(f64::from).collect()
    }

    #[test]
    fn serie_perfeita_tem_declive_um_e_ic_contendo_um() {
        let resultado = theil_sen(&crescente(1, 20)).expect("série válida");

        assert!((resultado.slope - 1.0).abs() < 1e-12);
        assert!((resultado.intercept - 1.0).abs() < 1e-12);
        assert!(resultado.slope_ic95.0 <= 1.0);
        assert!(resultado.slope_ic95.1 >= 1.0);
        assert_eq!(resultado.n, 20);
    }

    #[test]
    fn outlier_extremo_nao_distorce_o_declive() {
        let mut serie = crescente(1, 19);
        serie.push(1000.0);

        let resultado = theil_sen(&serie).expect("série válida");
        assert!((resultado.slope - 1.0).abs() < 1e-12);
        assert!((resultado.intercept - 1.0).abs() < 1e-12);
    }

    #[test]
    fn serie_com_menos_de_tres_pontos_e_erro() {
        let erro = theil_sen(&[1.0, 2.0]).expect_err("série curta");
        assert!(matches!(erro, AnalysisError::SerieCurta { n: 2 }));
    }
}
