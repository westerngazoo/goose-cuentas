//! Las estadísticas que el dueño copia de Instagram: dos CSV en el vault.
//!
//! Cada fila es una **medición**: lo que Instagram reportaba en su `fecha`. Se
//! agregan filas, nunca se reescriben (la historia es el dato). Una celda vacía
//! es «no se copió», no cero. El encabezado se compara letra por letra, así
//! que una columna movida a mano falla con su nombre en vez de leerse mal.
//!
//! New columns are only ever appended **at the end**: an older header is a
//! prefix of the current one, so its (shorter) rows still load, with the
//! missing cells empty. An older header is upgraded to the current one the
//! next time a row is appended, leaving the existing rows byte for byte.

use std::path::Path;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::registro::Registro;
use crate::slug_valido;

pub const ENCABEZADO_POSTS: &str = "fecha,cuenta,post,vistas,alcance,likes,comentarios,guardados,compartidos,seguidores_ganados,nota,salto_pct,medio_s,no_seguidores_pct";
/// The `posts.csv` header before the retention columns (2026-11-20): still
/// read, and upgraded to [`ENCABEZADO_POSTS`] on the next append.
pub const ENCABEZADO_POSTS_V1: &str = "fecha,cuenta,post,vistas,alcance,likes,comentarios,guardados,compartidos,seguidores_ganados,nota";
/// Every `posts.csv` header this version reads, the current one first. Each
/// older one must be a prefix of the newer ones.
const ENCABEZADOS_POSTS: [&str; 2] = [ENCABEZADO_POSTS, ENCABEZADO_POSTS_V1];
pub const ENCABEZADO_SEGUIDORES: &str = "fecha,cuenta,seguidores,nota";

/// Una medición de un post.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct Medicion {
    pub fecha: String,
    pub cuenta: String,
    pub post: String,
    pub vistas: Option<u64>,
    pub alcance: Option<u64>,
    pub likes: Option<u64>,
    pub comentarios: Option<u64>,
    pub guardados: Option<u64>,
    pub compartidos: Option<u64>,
    pub seguidores_ganados: Option<u64>,
    #[serde(default)]
    pub nota: String,
    /// The skip rate: % of viewers who left before 3 s (0 to 100).
    #[serde(default)]
    pub salto_pct: Option<f64>,
    /// The average watch time, in seconds (≥ 0).
    #[serde(default)]
    pub medio_s: Option<f64>,
    /// % of the views that came from non-followers (0 to 100).
    #[serde(default)]
    pub no_seguidores_pct: Option<f64>,
}

/// What a retention figure measures, and therefore its valid range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unidad {
    /// A percentage, from 0 to 100.
    Porcentaje,
    /// Seconds, ≥ 0.
    Segundos,
}

impl Unidad {
    fn admite(self, v: f64) -> bool {
        match self {
            Unidad::Porcentaje => (0.0..=100.0).contains(&v),
            Unidad::Segundos => v >= 0.0,
        }
    }

    fn rango(self) -> &'static str {
        match self {
            Unidad::Porcentaje => "un porcentaje de 0 a 100",
            Unidad::Segundos => "segundos, ≥ 0",
        }
    }
}

/// A retention figure that is not one: the typed error of [`leer_cifra`] and
/// [`validar_cifra`]. `campo` is where the user typed it (a flag or a column).
#[derive(Debug, Clone, PartialEq)]
pub enum CifraInvalida {
    /// Not a finite decimal number.
    NoEsNumero {
        campo: String,
        valor: String,
        unidad: Unidad,
    },
    /// A number outside the range of its unit (NaN and infinities included).
    FueraDeRango {
        campo: String,
        valor: f64,
        unidad: Unidad,
    },
}

impl std::fmt::Display for CifraInvalida {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CifraInvalida::NoEsNumero {
                campo,
                valor,
                unidad,
            } => write!(f, "{campo} «{valor}»: no es un número; {}", unidad.rango()),
            CifraInvalida::FueraDeRango {
                campo,
                valor,
                unidad,
            } => write!(f, "{campo} {valor}: fuera de rango; {}", unidad.rango()),
        }
    }
}

impl std::error::Error for CifraInvalida {}

/// Checks that `v` is finite and within the range of `unidad`.
///
/// Returns `v` (a negative zero as `0`), or [`CifraInvalida::FueraDeRango`].
pub fn validar_cifra(campo: &str, v: f64, unidad: Unidad) -> Result<f64, CifraInvalida> {
    if v.is_finite() && unidad.admite(v) {
        // `-0` is in range; store it as `0`
        Ok(v + 0.0)
    } else {
        Err(CifraInvalida::FueraDeRango {
            campo: campo.to_string(),
            valor: v,
            unidad,
        })
    }
}

/// Parses a retention figure as typed (`45`, `45.5`; surrounding spaces are
/// allowed and the decimal separator is `.`) and validates it with
/// [`validar_cifra`].
///
/// Errors: [`CifraInvalida::NoEsNumero`] if it is not a finite decimal number,
/// [`CifraInvalida::FueraDeRango`] if it is out of range.
pub fn leer_cifra(campo: &str, texto: &str, unidad: Unidad) -> Result<f64, CifraInvalida> {
    let v = texto
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| CifraInvalida::NoEsNumero {
            campo: campo.to_string(),
            valor: texto.to_string(),
            unidad,
        })?;
    validar_cifra(campo, v, unidad)
}

impl Medicion {
    /// Validates the retention figures that were copied, each named by its
    /// column.
    pub fn validar_retencion(&self) -> Result<(), CifraInvalida> {
        for (campo, v, unidad) in [
            ("salto_pct", self.salto_pct, Unidad::Porcentaje),
            ("medio_s", self.medio_s, Unidad::Segundos),
            (
                "no_seguidores_pct",
                self.no_seguidores_pct,
                Unidad::Porcentaje,
            ),
        ] {
            if let Some(v) = v {
                validar_cifra(campo, v, unidad)?;
            }
        }
        Ok(())
    }

    /// Whether any retention figure was copied.
    pub fn tiene_retencion(&self) -> bool {
        [self.salto_pct, self.medio_s, self.no_seguidores_pct]
            .iter()
            .any(Option::is_some)
    }

    /// The retention figures for a terminal line, «—» for one not copied:
    /// `salto 45.5 % · medio 8 s · no seg. 62 %`.
    pub fn retencion_texto(&self) -> String {
        format!(
            "salto {} · medio {} · no seg. {}",
            con_unidad(self.salto_pct, "%"),
            con_unidad(self.medio_s, "s"),
            con_unidad(self.no_seguidores_pct, "%")
        )
    }

    /// Las interacciones que se copiaron (likes, comentarios, guardados y
    /// compartidos); `None` si no se copió ninguna.
    pub fn interacciones(&self) -> Option<u64> {
        let v = [
            self.likes,
            self.comentarios,
            self.guardados,
            self.compartidos,
        ];
        v.iter()
            .any(Option::is_some)
            .then(|| v.iter().flatten().sum())
    }

    fn alguna_cifra(&self) -> bool {
        [
            self.vistas,
            self.alcance,
            self.likes,
            self.comentarios,
            self.guardados,
            self.compartidos,
            self.seguidores_ganados,
        ]
        .iter()
        .any(Option::is_some)
            || self.tiene_retencion()
    }
}

/// `45.5 %` (a narrow no-break space before the unit), or «—» if not copied.
pub fn con_unidad(v: Option<f64>, unidad: &str) -> String {
    v.map(|x| format!("{x}\u{202f}{unidad}"))
        .unwrap_or_else(|| "—".into())
}

/// Una medición de los seguidores de una cuenta.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Seguidores {
    pub fecha: String,
    pub cuenta: String,
    pub seguidores: u64,
    #[serde(default)]
    pub nota: String,
}

/// Las mediciones de los posts; sin archivo, ninguna.
pub fn leer_posts(ruta: &Path) -> Result<Vec<Medicion>, String> {
    leer(ruta, &ENCABEZADOS_POSTS)
}

/// Las mediciones de los seguidores; sin archivo, ninguna.
pub fn leer_seguidores(ruta: &Path) -> Result<Vec<Seguidores>, String> {
    leer(ruta, &[ENCABEZADO_SEGUIDORES])
}

/// Agrega una medición de un post, validada contra el registro.
pub fn agregar_medicion(ruta: &Path, m: &Medicion, registro: &Registro) -> Result<(), String> {
    validar_comun(&m.fecha, &m.cuenta, &m.nota, registro)?;
    if !slug_valido(&m.post) {
        return Err(format!(
            "post «{}»: el nombre de su carpeta (minúsculas, dígitos y guiones)",
            m.post
        ));
    }
    if !m.alguna_cifra() {
        return Err("la medición no trae ninguna cifra".into());
    }
    m.validar_retencion().map_err(|e| e.to_string())?;
    agregar(ruta, &ENCABEZADOS_POSTS, m)
}

/// Agrega una medición de seguidores, validada contra el registro.
pub fn agregar_seguidores(ruta: &Path, s: &Seguidores, registro: &Registro) -> Result<(), String> {
    validar_comun(&s.fecha, &s.cuenta, &s.nota, registro)?;
    agregar(ruta, &[ENCABEZADO_SEGUIDORES], s)
}

fn validar_comun(fecha: &str, cuenta: &str, nota: &str, registro: &Registro) -> Result<(), String> {
    if !guion_tablero::fecha::es_fecha(fecha) {
        return Err(format!("fecha «{fecha}»: AAAA-MM-DD"));
    }
    registro.cuenta(cuenta)?;
    if nota.contains(['\n', '\r']) {
        return Err("la nota es una línea".into());
    }
    Ok(())
}

fn leer<T: DeserializeOwned>(ruta: &Path, encabezados: &[&str]) -> Result<Vec<T>, String> {
    let texto = match std::fs::read_to_string(ruta) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{}: {e}", ruta.display())),
    };
    parse(&texto, encabezados).map_err(|e| format!("{}: {e}", ruta.display()))
}

/// Reads a CSV whose header is exactly one of `encabezados` (the current one
/// first, each older one a prefix of the newer ones; never empty).
///
/// A row may have as many cells as the file's header or as an older header;
/// the cells it lacks are empty. Any other count fails with its row number,
/// as does a header that is none of them.
pub fn parse<T: DeserializeOwned>(texto: &str, encabezados: &[&str]) -> Result<Vec<T>, String> {
    let primera = texto.lines().next().unwrap_or("").trim_end_matches('\r');
    if !encabezados.contains(&primera) {
        return Err(format!(
            "el encabezado es «{primera}»; debe ser «{}»",
            encabezados.first().copied().unwrap_or_default()
        ));
    }
    let columnas = |e: &str| e.split(',').count();
    let del_archivo = columnas(primera);
    let validas: Vec<usize> = encabezados
        .iter()
        .map(|e| columnas(e))
        .filter(|n| *n <= del_archivo)
        .collect();
    let mut lector = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(texto.as_bytes());
    let cabeza = lector.headers().map_err(|e| e.to_string())?.clone();
    lector
        .records()
        .enumerate()
        .map(|(i, r)| {
            let fila = i + 2;
            let r = r.map_err(|e| format!("fila {fila}: {e}"))?;
            if !validas.contains(&r.len()) {
                return Err(format!(
                    "fila {fila}: {} celdas; el encabezado tiene {del_archivo}",
                    r.len()
                ));
            }
            r.deserialize(Some(&cabeza))
                .map_err(|e| format!("fila {fila}: {e}"))
        })
        .collect()
}

/// `texto` with its first line replaced by the current header (`encabezados[0]`)
/// if it is an older one; the rows stay byte for byte. Anything else is
/// returned as is, for [`parse`] to report.
fn subir_encabezado(texto: &str, encabezados: &[&str]) -> String {
    let (primera, resto) = texto.split_once('\n').unwrap_or((texto, ""));
    match encabezados.split_first() {
        Some((actual, viejos)) if viejos.contains(&primera.trim_end_matches('\r')) => {
            format!("{actual}\n{resto}")
        }
        _ => texto.to_string(),
    }
}

/// Appends `fila` under `encabezados[0]` (never empty), upgrading an older
/// header first. The result is parsed back before it is written.
fn agregar<T: Serialize + DeserializeOwned>(
    ruta: &Path,
    encabezados: &[&str],
    fila: &T,
) -> Result<(), String> {
    let encabezado = encabezados.first().copied().unwrap_or_default();
    let mut actual = match std::fs::read_to_string(ruta) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => format!("{encabezado}\n"),
        Err(e) => return Err(format!("{}: {e}", ruta.display())),
    };
    if !actual.is_empty() && !actual.ends_with('\n') {
        actual.push('\n');
    }
    let mut actual = subir_encabezado(&actual, encabezados);
    let mut w = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(Vec::new());
    w.serialize(fila).map_err(|e| e.to_string())?;
    let bytes = w.into_inner().map_err(|e| e.to_string())?;
    actual.push_str(&String::from_utf8(bytes).map_err(|e| e.to_string())?);
    // lo que se escribe tiene que volver a leerse: nunca un CSV roto en el vault
    parse::<T>(&actual, encabezados).map_err(|e| format!("{}: {e}", ruta.display()))?;
    if let Some(dir) = ruta.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(ruta, actual).map_err(|e| format!("{}: {e}", ruta.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registro() -> Registro {
        Registro::parse(
            "version = 1\n[[cuenta]]\nslug = \"rotor\"\nnombre = \"Rotor\"\nrepo = \"r\"\n",
            Path::new("/v"),
            None,
        )
        .unwrap()
    }

    fn tmp(nombre: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("goose-cuentas-{nombre}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d.join("estadisticas/posts.csv")
    }

    #[test]
    fn agregar_y_leer_conserva_vacios_y_comas() {
        let ruta = tmp("csv");
        let m = Medicion {
            fecha: "2026-10-20".into(),
            cuenta: "rotor".into(),
            post: "deriva".into(),
            vistas: Some(1200),
            likes: Some(80),
            nota: "copiado de la app, sin Insights".into(),
            ..Default::default()
        };
        agregar_medicion(&ruta, &m, &registro()).unwrap();
        agregar_medicion(
            &ruta,
            &Medicion {
                vistas: Some(1500),
                ..m.clone()
            },
            &registro(),
        )
        .unwrap();
        let leidas = leer_posts(&ruta).unwrap();
        assert_eq!(leidas.len(), 2);
        assert_eq!(leidas[0], m);
        assert_eq!(leidas[0].alcance, None);
        assert_eq!(leidas[1].vistas, Some(1500));
        let texto = std::fs::read_to_string(&ruta).unwrap();
        assert!(texto.starts_with(ENCABEZADO_POSTS));
        assert!(texto.contains("\"copiado de la app, sin Insights\""));
    }

    #[test]
    fn rechaza_lo_que_no_es() {
        let ruta = tmp("malo");
        let r = registro();
        let base = Medicion {
            fecha: "2026-10-20".into(),
            cuenta: "rotor".into(),
            post: "deriva".into(),
            likes: Some(1),
            ..Default::default()
        };
        let f = |m: Medicion| agregar_medicion(&ruta, &m, &r).unwrap_err();
        assert!(f(Medicion {
            fecha: "20/10/2026".into(),
            ..base.clone()
        })
        .contains("AAAA-MM-DD"));
        assert!(f(Medicion {
            cuenta: "pmmo".into(),
            ..base.clone()
        })
        .contains("hay: rotor"));
        assert!(f(Medicion {
            post: "Deriva".into(),
            ..base.clone()
        })
        .contains("carpeta"));
        assert!(f(Medicion {
            likes: None,
            ..base.clone()
        })
        .contains("ninguna cifra"));
        assert!(f(Medicion {
            nota: "a\nb".into(),
            ..base
        })
        .contains("una línea"));
        assert!(!ruta.exists());
    }

    #[test]
    fn el_encabezado_movido_falla_con_nombre() {
        let e = parse::<Seguidores>("cuenta,fecha,seguidores,nota\n", &[ENCABEZADO_SEGUIDORES])
            .unwrap_err();
        assert!(e.contains("cuenta,fecha"));
        let e = parse::<Seguidores>(
            "fecha,cuenta,seguidores,nota\n2026-10-20,rotor,muchos,\n",
            &[ENCABEZADO_SEGUIDORES],
        )
        .unwrap_err();
        assert!(e.contains("fila 2"));
    }

    #[test]
    fn older_headers_are_prefixes_of_the_current_one() {
        for viejo in &ENCABEZADOS_POSTS[1..] {
            assert!(ENCABEZADO_POSTS.starts_with(&format!("{viejo},")));
        }
    }

    const FILA_V1: &str = "2026-10-20,rotor,deriva,1200,,80,,,,,\"de la app, sin Insights\"\n";

    #[test]
    fn an_old_format_csv_still_loads() {
        let texto = format!("{ENCABEZADO_POSTS_V1}\n{FILA_V1}");
        let ms = parse::<Medicion>(&texto, &ENCABEZADOS_POSTS).unwrap();
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].vistas, Some(1200));
        assert_eq!(ms[0].nota, "de la app, sin Insights");
        assert_eq!(ms[0].salto_pct, None);
        assert_eq!(ms[0].medio_s, None);
        assert_eq!(ms[0].no_seguidores_pct, None);
        // a header-only old file, like the owner's today, loads as no rows
        let vacio = format!("{ENCABEZADO_POSTS_V1}\n");
        assert!(parse::<Medicion>(&vacio, &ENCABEZADOS_POSTS)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn appending_upgrades_the_old_header_and_keeps_old_rows() {
        let ruta = tmp("v1");
        std::fs::create_dir_all(ruta.parent().unwrap()).unwrap();
        std::fs::write(&ruta, format!("{ENCABEZADO_POSTS_V1}\n{FILA_V1}")).unwrap();
        let m = Medicion {
            fecha: "2026-11-20".into(),
            cuenta: "rotor".into(),
            post: "deriva".into(),
            salto_pct: Some(45.5),
            medio_s: Some(8.0),
            no_seguidores_pct: Some(62.0),
            ..Default::default()
        };
        agregar_medicion(&ruta, &m, &registro()).unwrap();
        let texto = std::fs::read_to_string(&ruta).unwrap();
        assert_eq!(
            texto,
            format!("{ENCABEZADO_POSTS}\n{FILA_V1}2026-11-20,rotor,deriva,,,,,,,,,45.5,8.0,62.0\n")
        );
        let ms = leer_posts(&ruta).unwrap();
        assert_eq!(ms[0].salto_pct, None);
        assert_eq!(ms[1], m);
    }

    #[test]
    fn a_row_with_a_wrong_cell_count_fails_with_its_number() {
        // 12 cells: neither the old nor the current header
        let texto = format!("{ENCABEZADO_POSTS}\n{FILA_V1}2026-10-21,rotor,deriva,1,,,,,,,,9\n");
        let e = parse::<Medicion>(&texto, &ENCABEZADOS_POSTS).unwrap_err();
        assert!(e.contains("fila 3"), "{e}");
        // under the old header, a row of the new length is not valid either
        let largo = "2026-10-21,rotor,deriva,1,,,,,,,,9,8,7\n";
        let texto = format!("{ENCABEZADO_POSTS_V1}\n{largo}");
        assert!(parse::<Medicion>(&texto, &ENCABEZADOS_POSTS)
            .unwrap_err()
            .contains("fila 2"));
    }

    #[test]
    fn retention_figures_parse_and_validate() {
        assert_eq!(leer_cifra("--salto", "45.5", Unidad::Porcentaje), Ok(45.5));
        assert_eq!(
            leer_cifra("--salto", " 100 ", Unidad::Porcentaje),
            Ok(100.0)
        );
        assert_eq!(leer_cifra("--medio", "0", Unidad::Segundos), Ok(0.0));
        assert_eq!(leer_cifra("--medio", "-0", Unidad::Segundos), Ok(0.0));
        assert_eq!(leer_cifra("--medio", "3600", Unidad::Segundos), Ok(3600.0));
        for malo in ["", "abc", "45%", "4,5", "NaN", "inf"] {
            assert!(
                matches!(
                    leer_cifra("--salto", malo, Unidad::Porcentaje),
                    Err(CifraInvalida::NoEsNumero { .. })
                ),
                "{malo}"
            );
        }
        for (v, u) in [
            ("100.1", Unidad::Porcentaje),
            ("-1", Unidad::Porcentaje),
            ("-0.5", Unidad::Segundos),
        ] {
            assert!(matches!(
                leer_cifra("x", v, u),
                Err(CifraInvalida::FueraDeRango { .. })
            ));
        }
    }

    #[test]
    fn out_of_range_retention_is_rejected_before_writing() {
        let ruta = tmp("retencion-mala");
        let base = Medicion {
            fecha: "2026-11-20".into(),
            cuenta: "rotor".into(),
            post: "deriva".into(),
            ..Default::default()
        };
        for (m, campo) in [
            (
                Medicion {
                    salto_pct: Some(101.0),
                    ..base.clone()
                },
                "salto_pct",
            ),
            (
                Medicion {
                    medio_s: Some(-1.0),
                    ..base.clone()
                },
                "medio_s",
            ),
            (
                Medicion {
                    no_seguidores_pct: Some(f64::NAN),
                    ..base.clone()
                },
                "no_seguidores_pct",
            ),
        ] {
            let e = agregar_medicion(&ruta, &m, &registro()).unwrap_err();
            assert!(e.contains(campo) && e.contains("fuera de rango"), "{e}");
        }
        assert!(!ruta.exists());
    }

    #[test]
    fn interacciones() {
        let m = Medicion {
            likes: Some(10),
            guardados: Some(3),
            ..Default::default()
        };
        assert_eq!(m.interacciones(), Some(13));
        assert_eq!(Medicion::default().interacciones(), None);
    }
}
