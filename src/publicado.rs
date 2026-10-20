//! Marcar un post `publicado` en el README de su repo.
//!
//! Solo lo hace el dueño (o el agente cuando el dueño le dice que ya lo
//! subió): nada se marca publicado por inferencia. La línea `estado:` queda
//! como la escriben los repos de la casa, con lo anterior detrás:
//! `publicado (2026-10-21, https://…). Antes: listo (…)`.

use std::path::Path;

use guion_tablero::post::{leer_estado, Estado};

/// Reescribe la línea `estado:` del README y devuelve la línea nueva. Si ya
/// estaba publicado, solo agrega el enlace donde decía «enlace pendiente».
pub fn marcar(readme: &Path, fecha: &str, enlace: Option<&str>) -> Result<String, String> {
    if !guion_tablero::fecha::es_fecha(fecha) {
        return Err(format!("fecha «{fecha}»: AAAA-MM-DD"));
    }
    if let Some(e) = enlace {
        if !e.starts_with("https://") || e.contains(char::is_whitespace) {
            return Err(format!("enlace «{e}»: una URL https sin espacios"));
        }
    }
    let texto =
        std::fs::read_to_string(readme).map_err(|e| format!("{}: {e}", readme.display()))?;
    let nuevo =
        reescribir(&texto, fecha, enlace).map_err(|e| format!("{}: {e}", readme.display()))?;
    let linea = leer_estado(&nuevo).expect("la línea se acaba de escribir");
    std::fs::write(readme, &nuevo).map_err(|e| format!("{}: {e}", readme.display()))?;
    Ok(format!("{}{}", linea.prefijo, linea.valor))
}

/// El texto del README con la línea `estado:` reescrita.
pub fn reescribir(texto: &str, fecha: &str, enlace: Option<&str>) -> Result<String, String> {
    let l = leer_estado(texto).ok_or("el README no tiene línea `estado:`")?;
    let valor = if l.estado == Some(Estado::Publicado) {
        let e = enlace.ok_or("ya está publicado")?;
        if l.valor.contains(e) {
            return Err("ya tiene ese enlace".into());
        }
        if !l.valor.contains("enlace pendiente") {
            return Err("ya está publicado y no dice «enlace pendiente»; edítalo a mano".into());
        }
        l.valor.replacen("enlace pendiente", e, 1)
    } else {
        format!(
            "publicado ({fecha}, {}). Antes: {}",
            enlace.unwrap_or("enlace pendiente"),
            l.valor
        )
    };
    let mut out = String::with_capacity(texto.len() + 64);
    for (i, linea) in texto.split_inclusive('\n').enumerate() {
        if i == l.linea {
            let fin = &linea[linea.trim_end_matches(['\n', '\r']).len()..];
            out.push_str(&l.prefijo);
            out.push_str(&valor);
            out.push_str(fin);
        } else {
            out.push_str(linea);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const README: &str = "# ¿Qué es la deriva?\n\n- **estado:** listo (2026-10-16). Paquete en publicar/\n- **serie:** física\n";

    #[test]
    fn marca_y_conserva_lo_anterior() {
        let t = reescribir(
            README,
            "2026-10-21",
            Some("https://www.instagram.com/reel/abc/"),
        )
        .unwrap();
        assert_eq!(
            t.lines().nth(2).unwrap(),
            "- **estado:** publicado (2026-10-21, https://www.instagram.com/reel/abc/). Antes: listo (2026-10-16). Paquete en publicar/"
        );
        assert!(t.ends_with("- **serie:** física\n"));
        assert_eq!(leer_estado(&t).unwrap().estado, Some(Estado::Publicado));
    }

    #[test]
    fn el_enlace_llega_despues() {
        let t = reescribir(README, "2026-10-21", None).unwrap();
        assert!(t.contains("publicado (2026-10-21, enlace pendiente)"));
        assert!(reescribir(&t, "2026-10-22", None)
            .unwrap_err()
            .contains("ya está publicado"));
        let u = reescribir(&t, "2026-10-22", Some("https://x.y/z")).unwrap();
        assert!(u.contains("publicado (2026-10-21, https://x.y/z). Antes: listo"));
        assert!(reescribir(&u, "2026-10-22", Some("https://x.y/z"))
            .unwrap_err()
            .contains("ya tiene"));
    }

    #[test]
    fn sin_linea_de_estado() {
        assert!(reescribir("# x\n", "2026-10-21", None)
            .unwrap_err()
            .contains("estado"));
    }
}
