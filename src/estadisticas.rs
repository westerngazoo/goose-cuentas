//! Las estadísticas que el dueño copia de Instagram: dos CSV en el vault.
//!
//! Cada fila es una **medición**: lo que Instagram reportaba en su `fecha`. Se
//! agregan filas, nunca se reescriben (la historia es el dato). Una celda vacía
//! es «no se copió», no cero. El encabezado se compara letra por letra, así
//! que una columna movida a mano falla con su nombre en vez de leerse mal.

use std::path::Path;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::registro::Registro;
use crate::slug_valido;

pub const ENCABEZADO_POSTS: &str = "fecha,cuenta,post,vistas,alcance,likes,comentarios,guardados,compartidos,seguidores_ganados,nota";
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
}

impl Medicion {
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
    }
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
    leer(ruta, ENCABEZADO_POSTS)
}

/// Las mediciones de los seguidores; sin archivo, ninguna.
pub fn leer_seguidores(ruta: &Path) -> Result<Vec<Seguidores>, String> {
    leer(ruta, ENCABEZADO_SEGUIDORES)
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
    agregar(ruta, ENCABEZADO_POSTS, m)
}

/// Agrega una medición de seguidores, validada contra el registro.
pub fn agregar_seguidores(ruta: &Path, s: &Seguidores, registro: &Registro) -> Result<(), String> {
    validar_comun(&s.fecha, &s.cuenta, &s.nota, registro)?;
    agregar(ruta, ENCABEZADO_SEGUIDORES, s)
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

fn leer<T: DeserializeOwned>(ruta: &Path, encabezado: &str) -> Result<Vec<T>, String> {
    let texto = match std::fs::read_to_string(ruta) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{}: {e}", ruta.display())),
    };
    parse(&texto, encabezado).map_err(|e| format!("{}: {e}", ruta.display()))
}

/// Lee un CSV con el encabezado exacto.
pub fn parse<T: DeserializeOwned>(texto: &str, encabezado: &str) -> Result<Vec<T>, String> {
    let primera = texto.lines().next().unwrap_or("").trim_end_matches('\r');
    if primera != encabezado {
        return Err(format!(
            "el encabezado es «{primera}»; debe ser «{encabezado}»"
        ));
    }
    let mut lector = csv::Reader::from_reader(texto.as_bytes());
    lector
        .deserialize()
        .enumerate()
        .map(|(i, f)| f.map_err(|e| format!("fila {}: {e}", i + 2)))
        .collect()
}

fn agregar<T: Serialize + DeserializeOwned>(
    ruta: &Path,
    encabezado: &str,
    fila: &T,
) -> Result<(), String> {
    let mut actual = match std::fs::read_to_string(ruta) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => format!("{encabezado}\n"),
        Err(e) => return Err(format!("{}: {e}", ruta.display())),
    };
    if !actual.is_empty() && !actual.ends_with('\n') {
        actual.push('\n');
    }
    let mut w = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(Vec::new());
    w.serialize(fila).map_err(|e| e.to_string())?;
    let bytes = w.into_inner().map_err(|e| e.to_string())?;
    actual.push_str(&String::from_utf8(bytes).map_err(|e| e.to_string())?);
    // lo que se escribe tiene que volver a leerse: nunca un CSV roto en el vault
    parse::<T>(&actual, encabezado).map_err(|e| format!("{}: {e}", ruta.display()))?;
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
        let e = parse::<Seguidores>("cuenta,fecha,seguidores,nota\n", ENCABEZADO_SEGUIDORES)
            .unwrap_err();
        assert!(e.contains("cuenta,fecha"));
        let e = parse::<Seguidores>(
            "fecha,cuenta,seguidores,nota\n2026-10-20,rotor,muchos,\n",
            ENCABEZADO_SEGUIDORES,
        )
        .unwrap_err();
        assert!(e.contains("fila 2"));
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
