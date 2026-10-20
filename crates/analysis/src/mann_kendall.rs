use serde::Serialize;

use crate::errors::AnalysisError;

/// Direção de uma tendência estatisticamente significativa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Direcao {
    /// Tendência crescente.
    Crescente,
    /// Tendência decrescente.
    Decrescente,
    /// Sem evidência de tendência ao nível de significância pedido.
    Estavel,
}

/// Resultado do teste de Mann-Kendall.
#[derive(Debug, Clone, Serialize)]
pub struct ResultadoMK {
    /// Estatística S (soma dos sinais dos pares).
    pub s: f64,
    /// Tau de Kendall, em `[-1, 1]`.
    pub tau: f64,
    /// p-value bicaudal.
    pub p_value: f64,
    /// Número de observações.
    pub n: usize,
    /// Direção inferida ao nível de significância pedido.
    pub direcao: Direcao,
    /// `true` quando `n >= 10`.
    pub amostra_suficiente: bool,
}

/// Teste não-paramétrico de Mann-Kendall para tendência monótona.
///
/// A variância de `S` inclui a correção para empates:
///
/// `Var(S) = [n(n-1)(2n+5) - Σ t_p(t_p-1)(2t_p+5)] / 18`
///
/// onde `t_p` é o tamanho de cada grupo de valores iguais. O p-value é
/// bicaudal e usa uma aproximação da normal padrão com erro máximo da ordem
/// de `1e-7`.
pub fn mann_kendall(serie: &[f64], nivel_significancia: f64) -> Result<ResultadoMK, AnalysisError> {
    let n = serie.len();
    if n < 3 {
        return Err(AnalysisError::SerieCurta { n });
    }

    let s = estatistica_s(serie);
    let var_s = variancia_s(serie);

    let z = if var_s <= 0.0 {
        0.0
    } else if s > 0.0 {
        (s - 1.0) / var_s.sqrt()
    } else if s < 0.0 {
        (s + 1.0) / var_s.sqrt()
    } else {
        0.0
    };

    let p_value = (2.0 * (1.0 - normal_cdf(z.abs()))).clamp(0.0, 1.0);
    let tau = s / ((n as f64) * ((n - 1) as f64) / 2.0);
    let direcao = if p_value < nivel_significancia {
        if s > 0.0 {
            Direcao::Crescente
        } else if s < 0.0 {
            Direcao::Decrescente
        } else {
            Direcao::Estavel
        }
    } else {
        Direcao::Estavel
    };

    Ok(ResultadoMK {
        s,
        tau,
        p_value,
        n,
        direcao,
        amostra_suficiente: n >= 10,
    })
}

fn estatistica_s(serie: &[f64]) -> f64 {
    let mut s = 0.0;
    for i in 0..serie.len() {
        for j in (i + 1)..serie.len() {
            let diferenca = serie[j] - serie[i];
            // `f64::signum(0.0)` devolve 1.0 em Rust; empates têm de
            // contribuir com zero, não com +1.
            s += if diferenca > 0.0 {
                1.0
            } else if diferenca < 0.0 {
                -1.0
            } else {
                0.0
            };
        }
    }
    s
}

/// Variância de `S` com correção para empates.
///
/// Partilhada com o Theil-Sen para a construção do intervalo de confiança
/// analítico do declive.
pub(crate) fn variancia_s(serie: &[f64]) -> f64 {
    let n = serie.len();
    if n < 2 {
        return 0.0;
    }

    let mut variancia = (n as f64) * ((n - 1) as f64) * ((2 * n + 5) as f64) / 18.0;

    let mut ordenada = serie.to_vec();
    ordenada.sort_by(f64::total_cmp);
    let mut i = 0;
    while i < n {
        let mut j = i + 1;
        while j < n && ordenada[j] == ordenada[i] {
            j += 1;
        }
        let t = (j - i) as f64;
        if t > 1.0 {
            variancia -= t * (t - 1.0) * (2.0 * t + 5.0) / 18.0;
        }
        i = j;
    }

    variancia
}

fn normal_cdf(x: f64) -> f64 {
    0.5 * (1.0 + erf(x / std::f64::consts::SQRT_2))
}

/// Aproximação de `erf` de Abramowitz-Stegun (7.1.26), erro máximo ~1.5e-7.
fn erf(x: f64) -> f64 {
    let sinal = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();

    let p = 0.327_591_1;
    let a1 = 0.254_829_592;
    let a2 = -0.284_496_736;
    let a3 = 1.421_413_741;
    let a4 = -1.453_152_027;
    let a5 = 1.061_405_429;

    let t = 1.0 / (1.0 + p * x);
    let polinomio = ((((a5 * t + a4) * t + a3) * t + a2) * t + a1) * t;
    sinal * (1.0 - polinomio * (-x * x).exp())
}

#[cfg(test)]
mod tests {
    use super::{Direcao, mann_kendall};
    use crate::errors::AnalysisError;

    fn crescente(inicio: i32, fim: i32) -> Vec<f64> {
        (inicio..=fim).map(f64::from).collect()
    }

    #[test]
    fn serie_crescente_perfeita() {
        let resultado = mann_kendall(&crescente(1, 20), 0.05).expect("série válida");

        // Série estritamente crescente: S = n(n-1)/2 e tau = 1.
        assert!((resultado.tau - 1.0).abs() < 1e-12);
        assert!(resultado.s > 0.0);
        assert!(resultado.p_value < 0.001);
        assert_eq!(resultado.direcao, Direcao::Crescente);
        assert!(resultado.amostra_suficiente);
    }

    #[test]
    fn serie_decrescente_perfeita() {
        let serie: Vec<f64> = (1..=20).rev().map(f64::from).collect();
        let resultado = mann_kendall(&serie, 0.05).expect("série válida");

        assert!((resultado.tau + 1.0).abs() < 1e-12);
        assert!(resultado.s < 0.0);
        assert_eq!(resultado.direcao, Direcao::Decrescente);
    }

    #[test]
    fn serie_estacionaria_com_ruido_deterministico() {
        let serie = [5.0, 3.0, 7.0, 4.0, 6.0, 5.0, 5.0, 4.0, 6.0, 5.0, 5.0, 4.0];
        let resultado = mann_kendall(&serie, 0.05).expect("série válida");

        assert!(resultado.p_value > 0.05);
        assert_eq!(resultado.direcao, Direcao::Estavel);
    }

    #[test]
    fn outlier_extremo_nao_inverte_a_direcao() {
        // OLS seria distorcido pelo valor 1000; o MK é robusto a outliers.
        let mut serie = crescente(1, 19);
        serie.push(1000.0);

        let resultado = mann_kendall(&serie, 0.05).expect("série válida");
        assert_eq!(resultado.direcao, Direcao::Crescente);
        assert!(resultado.s > 0.0);
    }

    #[test]
    fn empates_usam_correcao_de_variancia() {
        let serie = [1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 3.0, 3.0, 3.0];
        let resultado = mann_kendall(&serie, 0.05).expect("série válida");

        // Cálculo manual: n=9, S=27, Var(S)=81, Z=26/9, p≈0.00387 e
        // tau=27/36=0.75. A correção de ties reduz a variância de 92 para 81.
        assert!((resultado.tau - 0.75).abs() < 1e-12);
        assert!((resultado.p_value - 0.00387).abs() < 1e-4);
        assert_eq!(resultado.direcao, Direcao::Crescente);
    }

    #[test]
    fn serie_com_menos_de_tres_pontos_e_erro() {
        let erro = mann_kendall(&[1.0, 2.0], 0.05).expect_err("série curta");
        assert!(matches!(erro, AnalysisError::SerieCurta { n: 2 }));
    }

    #[test]
    fn serie_curta_e_valida_mas_sinaliza_amostra_insuficiente() {
        let resultado = mann_kendall(&[1.0, 2.0, 3.0], 0.05).expect("série válida");
        assert!(!resultado.amostra_suficiente);
    }
}
