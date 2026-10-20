//! El tablero de administración: por cuenta, sus posts (de su repo, vía
//! guion-tablero) junto con las estadísticas del vault.
//!
//! De cada post cuenta la medición más reciente (por fecha; a la misma fecha,
//! la última fila). Las sumas son de esas mediciones. La interacción es
//! (likes + comentarios + guardados + compartidos) ÷ vistas, solo sobre los
//! posts que tienen vistas: es una razón calculada aquí con números que
//! reporta Instagram.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use guion_tablero::post::Estado;
use guion_tablero::tablero::Tablero;
use serde::Serialize;

use crate::estadisticas::{Medicion, Seguidores};
use crate::registro::Registro;

/// Un post con números o con estado.
#[derive(Debug, Clone, Serialize)]
pub struct PostResumen {
    pub post: String,
    pub titulo: Option<String>,
    /// El estado de su README; `None` si el post no está en el repo.
    pub estado: Option<String>,
    /// La medición más reciente.
    pub ultima: Option<Medicion>,
    pub mediciones: usize,
}

/// Una cuenta.
#[derive(Debug, Clone, Serialize)]
pub struct CuentaResumen {
    pub slug: String,
    pub nombre: String,
    pub instagram: String,
    pub repo: String,
    pub seguidores: Option<u64>,
    pub seguidores_fecha: Option<String>,
    /// Contra la medición anterior de seguidores.
    pub seguidores_cambio: Option<i64>,
    pub seguidores_desde: Option<String>,
    pub vistas: u64,
    pub likes: u64,
    pub seguidores_ganados: u64,
    pub interaccion: Option<f64>,
    /// Posts por estado, en el orden del ciclo (solo los que tienen alguno).
    pub estados: Vec<(String, usize)>,
    /// Los posts con números, de más a menos vistas.
    pub posts: Vec<PostResumen>,
    /// Publicados sin ninguna medición: lo que falta copiar.
    pub faltan: Vec<String>,
    pub avisos: Vec<String>,
}

/// Todas las cuentas.
#[derive(Debug, Clone, Serialize)]
pub struct Resumen {
    pub fecha: String,
    pub cuentas: Vec<CuentaResumen>,
}

/// Lee el tablero de cada repo del registro por separado: un repo que no se
/// puede leer deja su error, no tumba a los demás.
pub fn leer_tableros(registro: &Registro) -> Vec<(PathBuf, Result<Tablero, String>)> {
    registro
        .repos()
        .into_iter()
        .map(|r| {
            let t = Tablero::leer(std::slice::from_ref(&r), &|_: &Path| None);
            (r, t)
        })
        .collect()
}

/// Arma el resumen.
pub fn armar(
    registro: &Registro,
    mediciones: &[Medicion],
    seguidores: &[Seguidores],
    tableros: &[(PathBuf, Result<Tablero, String>)],
    fecha: &str,
) -> Resumen {
    let cuentas = registro
        .cuentas
        .iter()
        .map(|c| {
            let repo = registro.repo(c);
            let mut avisos = Vec::new();
            let posts_repo = match tableros.iter().find(|(r, _)| *r == repo) {
                Some((_, Ok(t))) => match t.cuenta(&c.nombre) {
                    Some(tc) => tc.posts.clone(),
                    None => {
                        avisos.push(format!(
                            "«{}» no está en el guion-cuentas.toml de {}",
                            c.nombre,
                            repo.display()
                        ));
                        Vec::new()
                    }
                },
                Some((_, Err(e))) => {
                    avisos.push(format!("no se pudo leer {}: {e}", repo.display()));
                    Vec::new()
                }
                None => Vec::new(),
            };

            // la medición más reciente de cada post
            let mut ultimas: BTreeMap<&str, (&Medicion, usize)> = BTreeMap::new();
            for m in mediciones.iter().filter(|m| m.cuenta == c.slug) {
                let e = ultimas.entry(&m.post).or_insert((m, 0));
                if m.fecha >= e.0.fecha {
                    e.0 = m;
                }
                e.1 += 1;
            }

            let mut posts: Vec<PostResumen> = ultimas
                .iter()
                .map(|(nombre, (m, n))| {
                    let p = posts_repo.iter().find(|p| p.nombre == *nombre);
                    PostResumen {
                        post: nombre.to_string(),
                        titulo: p.and_then(|p| p.titulo.clone()),
                        estado: p.and_then(|p| p.estado).map(|e| e.nombre().to_string()),
                        ultima: Some((*m).clone()),
                        mediciones: *n,
                    }
                })
                .collect();
            posts.sort_by(|a, b| {
                let v = |p: &PostResumen| p.ultima.as_ref().and_then(|m| m.vistas).unwrap_or(0);
                v(b).cmp(&v(a)).then_with(|| a.post.cmp(&b.post))
            });

            let faltan = posts_repo
                .iter()
                .filter(|p| p.estado == Some(Estado::Publicado))
                .filter(|p| !ultimas.contains_key(p.nombre.as_str()))
                .map(|p| p.nombre.clone())
                .collect();

            let estados = Estado::TODOS
                .iter()
                .map(|e| {
                    (
                        e.nombre().to_string(),
                        posts_repo.iter().filter(|p| p.estado == Some(*e)).count(),
                    )
                })
                .filter(|(_, n)| *n > 0)
                .collect();

            let ms: Vec<&Medicion> = posts.iter().filter_map(|p| p.ultima.as_ref()).collect();
            let vistas: u64 = ms.iter().filter_map(|m| m.vistas).sum();
            let likes = ms.iter().filter_map(|m| m.likes).sum();
            let seguidores_ganados = ms.iter().filter_map(|m| m.seguidores_ganados).sum();
            let con_vistas: Vec<&&Medicion> = ms
                .iter()
                .filter(|m| m.vistas.is_some_and(|v| v > 0))
                .collect();
            let base: u64 = con_vistas.iter().filter_map(|m| m.vistas).sum();
            let interaccion = (base > 0).then(|| {
                let i: u64 = con_vistas.iter().filter_map(|m| m.interacciones()).sum();
                i as f64 / base as f64
            });

            let mut segs: Vec<&Seguidores> =
                seguidores.iter().filter(|s| s.cuenta == c.slug).collect();
            segs.sort_by(|a, b| a.fecha.cmp(&b.fecha)); // estable: a la misma fecha, la última fila queda al final
            let ultima_s = segs.last();
            // el cambio es contra la medición anterior de otro día
            let anterior_s =
                ultima_s.and_then(|u| segs.iter().rev().find(|s| s.fecha < u.fecha).copied());

            CuentaResumen {
                slug: c.slug.clone(),
                nombre: c.nombre.clone(),
                instagram: c.instagram.clone(),
                repo: repo.display().to_string(),
                seguidores: ultima_s.map(|s| s.seguidores),
                seguidores_fecha: ultima_s.map(|s| s.fecha.clone()),
                seguidores_cambio: ultima_s
                    .zip(anterior_s)
                    .map(|(u, a)| u.seguidores as i64 - a.seguidores as i64),
                seguidores_desde: anterior_s.map(|s| s.fecha.clone()),
                vistas,
                likes,
                seguidores_ganados,
                interaccion,
                estados,
                posts,
                faltan,
                avisos,
            }
        })
        .collect();
    Resumen {
        fecha: fecha.to_string(),
        cuentas,
    }
}

fn miles(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('\u{202f}');
        }
        out.push(ch);
    }
    out
}

fn celda(v: Option<u64>) -> String {
    v.map(miles).unwrap_or_else(|| "—".into())
}

fn porcentaje(x: f64) -> String {
    format!("{:.1}\u{202f}%", x * 100.0)
}

impl Resumen {
    pub fn json(&self) -> String {
        let mut s = serde_json::to_string_pretty(self).expect("el resumen se serializa");
        s.push('\n');
        s
    }

    /// Para la terminal.
    pub fn texto(&self) -> String {
        let mut o = String::new();
        for c in &self.cuentas {
            let handle = if c.instagram.is_empty() {
                "(usuario por reservar)".to_string()
            } else {
                format!("@{}", c.instagram)
            };
            o.push_str(&format!("■ {} [{}] {}\n", c.nombre, c.slug, handle));
            let seg = match (c.seguidores, &c.seguidores_fecha) {
                (Some(n), Some(f)) => {
                    let cambio = c
                        .seguidores_cambio
                        .zip(c.seguidores_desde.as_ref())
                        .map(|(d, desde)| format!(" ({d:+} desde el {desde})"))
                        .unwrap_or_default();
                    format!("{} seguidores el {f}{cambio}", miles(n))
                }
                _ => "seguidores: sin medición".into(),
            };
            o.push_str(&format!("  {seg}\n"));
            let estados: Vec<String> = c.estados.iter().map(|(k, n)| format!("{k} {n}")).collect();
            if !estados.is_empty() {
                o.push_str(&format!("  posts: {}\n", estados.join(" · ")));
            }
            if !c.posts.is_empty() {
                let inter = c.interaccion.map(porcentaje).unwrap_or_else(|| "—".into());
                o.push_str(&format!(
                    "  posts con números: {} · {} vistas · {} likes · interacción {inter}\n",
                    c.posts.len(),
                    miles(c.vistas),
                    miles(c.likes)
                ));
                for p in c.posts.iter().take(5) {
                    let m = p
                        .ultima
                        .as_ref()
                        .expect("un post con números tiene medición");
                    o.push_str(&format!(
                        "    {:<24} {:>9} vistas {:>7} likes  ({})\n",
                        p.post,
                        celda(m.vistas),
                        celda(m.likes),
                        m.fecha
                    ));
                }
            }
            if !c.faltan.is_empty() {
                o.push_str(&format!(
                    "  publicados sin números: {}\n",
                    c.faltan.join(", ")
                ));
            }
            for a in &c.avisos {
                o.push_str(&format!("  aviso: {a}\n"));
            }
            o.push('\n');
        }
        o
    }

    /// La nota del vault (`cuentas/resumen.md`).
    pub fn markdown(&self) -> String {
        let mut o = format!(
            "---\ngenerado: {}\n---\n\n# Resumen de las cuentas\n\n\
             > Generado por `cuentas nota`; no lo edites a mano. Los números los \
             reporta Instagram y los copia el dueño (la fecha de cada uno es la de \
             su medición); la interacción es (likes + comentarios + guardados + \
             compartidos) ÷ vistas, calculada aquí.\n\n",
            self.fecha
        );
        o.push_str(
            "| Cuenta | Seguidores | Cambio | Vistas | Likes | Interacción | Sin números |\n",
        );
        o.push_str("|---|--:|--:|--:|--:|--:|--:|\n");
        for c in &self.cuentas {
            o.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} |\n",
                c.nombre,
                celda(c.seguidores),
                c.seguidores_cambio
                    .map(|d| format!("{d:+}"))
                    .unwrap_or_else(|| "—".into()),
                celda((!c.posts.is_empty()).then_some(c.vistas)),
                celda((!c.posts.is_empty()).then_some(c.likes)),
                c.interaccion.map(porcentaje).unwrap_or_else(|| "—".into()),
                c.faltan.len()
            ));
        }
        for c in &self.cuentas {
            o.push_str(&format!("\n## {}\n\n", c.nombre));
            if !c.instagram.is_empty() {
                o.push_str(&format!(
                    "[@{0}](https://www.instagram.com/{0}/)\n\n",
                    c.instagram
                ));
            }
            if !c.estados.is_empty() {
                let e: Vec<String> = c.estados.iter().map(|(k, n)| format!("{k} {n}")).collect();
                o.push_str(&format!("Posts: {}.\n\n", e.join(" · ")));
            }
            if c.posts.is_empty() {
                o.push_str("Sin mediciones todavía.\n");
            } else {
                o.push_str("| Post | Fecha | Vistas | Alcance | Likes | Coment. | Guard. | Compart. | Seguidores |\n");
                o.push_str("|---|---|--:|--:|--:|--:|--:|--:|--:|\n");
                for p in &c.posts {
                    let m = p
                        .ultima
                        .as_ref()
                        .expect("un post con números tiene medición");
                    let nombre = p
                        .titulo
                        .as_deref()
                        .map(|t| format!("{} ({})", t, p.post))
                        .unwrap_or_else(|| p.post.clone());
                    o.push_str(&format!(
                        "| {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
                        nombre.replace('|', "\\|"),
                        m.fecha,
                        celda(m.vistas),
                        celda(m.alcance),
                        celda(m.likes),
                        celda(m.comentarios),
                        celda(m.guardados),
                        celda(m.compartidos),
                        celda(m.seguidores_ganados)
                    ));
                }
            }
            if !c.faltan.is_empty() {
                o.push_str(&format!(
                    "\nPublicados sin números: {}.\n",
                    c.faltan.join(", ")
                ));
            }
            for a in &c.avisos {
                o.push_str(&format!("\n> Aviso: {a}\n"));
            }
        }
        o
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registro() -> Registro {
        Registro::parse(
            "version = 1\n[[cuenta]]\nslug = \"rotor\"\nnombre = \"Rotor\"\ninstagram = \"rotorfisicoracing\"\nrepo = \"no-existe\"\n",
            Path::new("/nada"),
            None,
        )
        .unwrap()
    }

    fn m(fecha: &str, post: &str, vistas: u64, likes: u64) -> Medicion {
        Medicion {
            fecha: fecha.into(),
            cuenta: "rotor".into(),
            post: post.into(),
            vistas: Some(vistas),
            likes: Some(likes),
            ..Default::default()
        }
    }

    #[test]
    fn la_ultima_medicion_manda_y_se_ordena_por_vistas() {
        let ms = [
            m("2026-10-21", "deriva", 1500, 90),
            m("2026-10-18", "deriva", 900, 50),
            m("2026-10-20", "presion", 4000, 100),
            m("2026-10-20", "presion", 4100, 110), // misma fecha: la última fila
        ];
        let s = [
            Seguidores {
                fecha: "2026-10-01".into(),
                cuenta: "rotor".into(),
                seguidores: 300,
                nota: String::new(),
            },
            Seguidores {
                fecha: "2026-10-20".into(),
                cuenta: "rotor".into(),
                seguidores: 342,
                nota: String::new(),
            },
        ];
        let r = armar(&registro(), &ms, &s, &[], "2026-10-21");
        let c = &r.cuentas[0];
        assert_eq!(c.posts[0].post, "presion");
        assert_eq!(c.posts[0].ultima.as_ref().unwrap().vistas, Some(4100));
        assert_eq!(c.posts[1].mediciones, 2);
        assert_eq!(c.vistas, 5600);
        assert_eq!(c.likes, 200);
        assert!((c.interaccion.unwrap() - 200.0 / 5600.0).abs() < 1e-12);
        assert_eq!(c.seguidores, Some(342));
        assert_eq!(c.seguidores_cambio, Some(42));
        let t = r.texto();
        assert!(
            t.contains("342 seguidores el 2026-10-20 (+42 desde el 2026-10-01)"),
            "{t}"
        );
        assert!(t.contains("5\u{202f}600 vistas"), "{t}");
        let md = r.markdown();
        assert!(
            md.contains("| Rotor | 342 | +42 | 5\u{202f}600 | 200 | 3.6\u{202f}% | 0 |"),
            "{md}"
        );
    }

    #[test]
    fn el_cambio_es_contra_otro_dia() {
        let s = |f: &str, n| Seguidores {
            fecha: f.into(),
            cuenta: "rotor".into(),
            seguidores: n,
            nota: String::new(),
        };
        let r = armar(
            &registro(),
            &[],
            &[
                s("2026-10-01", 300),
                s("2026-10-20", 380),
                s("2026-10-20", 390),
            ],
            &[],
            "2026-10-21",
        );
        let c = &r.cuentas[0];
        assert_eq!(c.seguidores, Some(390));
        assert_eq!(c.seguidores_cambio, Some(90));
        assert_eq!(c.seguidores_desde.as_deref(), Some("2026-10-01"));
        let r = armar(&registro(), &[], &[s("2026-10-20", 390)], &[], "2026-10-21");
        assert_eq!(r.cuentas[0].seguidores_cambio, None);
    }

    #[test]
    fn sin_numeros_no_inventa_ceros() {
        let r = armar(&registro(), &[], &[], &[], "2026-10-21");
        let c = &r.cuentas[0];
        assert_eq!(c.seguidores, None);
        assert_eq!(c.interaccion, None);
        assert!(r.texto().contains("seguidores: sin medición"));
        assert!(r.markdown().contains("Sin mediciones todavía."));
        assert!(r.markdown().contains("| Rotor | — | — | — | — | — | 0 |"));
    }

    #[test]
    fn miles_con_espacio_fino() {
        assert_eq!(miles(7), "7");
        assert_eq!(miles(1234567), "1\u{202f}234\u{202f}567");
    }
}
