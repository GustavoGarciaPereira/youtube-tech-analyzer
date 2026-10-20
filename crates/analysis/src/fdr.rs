use crate::errors::AnalysisError;

/// Ajusta p-values pelo procedimento de Benjamini-Hochberg (FDR).
///
/// Devolve os p-values ajustados na mesma ordem da entrada. O parâmetro
/// `alpha` fica disponível para a camada chamadora decidir rejeição; os
/// valores ajustados em si não dependem dele.
///
/// Algoritmo:
/// 1. ordenar os p-values;
/// 2. calcular `p_adj_i = min(1, p_i * m / (i+1))`;
/// 3. percorrer de trás para a frente, garantindo que os p-values ajustados
///    são não-decrescentes na ordem ordenada (step-up).
pub fn benjamini_hochberg(p_values: &[f64], _alpha: f64) -> Result<Vec<f64>, AnalysisError> {
    for &p in p_values {
        if !p.is_finite() || !(0.0..=1.0).contains(&p) {
            return Err(AnalysisError::PValueInvalido(p));
        }
    }

    let m = p_values.len();
    if m == 0 {
        return Ok(Vec::new());
    }

    let mut indices: Vec<usize> = (0..m).collect();
    indices.sort_by(|&a, &b| p_values[a].total_cmp(&p_values[b]));

    let mut ajustados = vec![0.0; m];
    let mut minimo = 1.0_f64;

    for (rank, &indice) in indices.iter().enumerate().rev() {
        let valor = (p_values[indice] * m as f64 / (rank + 1) as f64).min(1.0);
        minimo = minimo.min(valor);
        ajustados[indice] = minimo;
    }

    Ok(ajustados)
}

#[cfg(test)]
mod tests {
    use super::benjamini_hochberg;
    use crate::errors::AnalysisError;

    #[test]
    fn fdr_esperado_para_cinco_p_values() {
        let p_values = [0.001, 0.01, 0.04, 0.5, 0.9];
        let ajustados = benjamini_hochberg(&p_values, 0.05).expect("p-values válidos");

        // BH: [0.001*5/1, 0.01*5/2, 0.04*5/3, 0.5*5/4, 0.9*5/5].
        assert!((ajustados[0] - 0.005).abs() < 1e-12);
        assert!((ajustados[1] - 0.025).abs() < 1e-12);
        assert!((ajustados[2] - 0.066_666_666_666_666_67).abs() < 1e-12);
        assert!((ajustados[3] - 0.625).abs() < 1e-12);
        assert!((ajustados[4] - 0.9).abs() < 1e-12);

        assert!(ajustados.windows(2).all(|janela| janela[0] <= janela[1]));
        assert_eq!(ajustados.iter().filter(|p| **p < 0.05).count(), 2);
    }

    #[test]
    fn fdr_preserva_a_ordem_original() {
        let p_values = [0.04, 0.001, 0.5, 0.01, 0.9];
        let ajustados = benjamini_hochberg(&p_values, 0.05).expect("p-values válidos");

        assert!((ajustados[0] - 0.066_666_666_666_666_67).abs() < 1e-12);
        assert!((ajustados[1] - 0.005).abs() < 1e-12);
        assert!((ajustados[2] - 0.625).abs() < 1e-12);
        assert!((ajustados[3] - 0.025).abs() < 1e-12);
        assert!((ajustados[4] - 0.9).abs() < 1e-12);
    }

    #[test]
    fn fdr_com_input_vazio_devolve_vec_vazio() {
        let ajustados = benjamini_hochberg(&[], 0.05).expect("input vazio");
        assert!(ajustados.is_empty());
    }

    #[test]
    fn fdr_rejeita_p_values_invalidos() {
        assert!(matches!(
            benjamini_hochberg(&[-0.1], 0.05),
            Err(AnalysisError::PValueInvalido(_))
        ));
        assert!(matches!(
            benjamini_hochberg(&[1.5], 0.05),
            Err(AnalysisError::PValueInvalido(_))
        ));
        assert!(matches!(
            benjamini_hochberg(&[f64::NAN], 0.05),
            Err(AnalysisError::PValueInvalido(_))
        ));
    }
}
