//! Lo que se hace con las cuentas, lo mismo desde la línea de comandos que
//! desde la interfaz: leer el estado, capturar números, pedir un reel y
//! marcar un post publicado. Cada escritura reescribe `resumen.md`.

use std::path::{Path, PathBuf};

use guion_tablero::encargos::Encargo;
use guion_tablero::tablero::Tablero;
use serde::Serialize;

use crate::estadisticas::{self, Medicion, Seguidores};
use crate::publicado;
use crate::registro::Registro;
use crate::resumen::{self, Resumen};
use crate::slug_valido;
use crate::vault::Vault;

/// El tablero de un repo (sin medir duraciones: no hace falta ffmpeg).
pub fn tablero_de(repo: &Path) -> Result<Tablero, String> {
    Tablero::leer(&[repo.to_path_buf()], &|_: &Path| None)
}

/// El resumen de todas las cuentas.
pub fn resumen(vault: &Vault, hoy: &str) -> Result<Resumen, String> {
    let reg = Registro::leer(&vault.registro())?;
    let ms = estadisticas::leer_posts(&vault.posts_csv())?;
    let ss = estadisticas::leer_seguidores(&vault.seguidores_csv())?;
    let tableros = resumen::leer_tableros(&reg);
    Ok(resumen::armar(&reg, &ms, &ss, &tableros, hoy))
}

/// Reescribe `resumen.md` y devuelve su ruta.
pub fn escribir_nota(vault: &Vault, hoy: &str) -> Result<PathBuf, String> {
    let r = resumen(vault, hoy)?;
    let ruta = vault.resumen_md();
    std::fs::write(&ruta, r.markdown()).map_err(|e| format!("{}: {e}", ruta.display()))?;
    Ok(ruta)
}

/// Agrega una medición de un post y reescribe la nota.
pub fn medir_post(vault: &Vault, m: &Medicion, hoy: &str) -> Result<PathBuf, String> {
    let reg = Registro::leer(&vault.registro())?;
    estadisticas::agregar_medicion(&vault.posts_csv(), m, &reg)?;
    escribir_nota(vault, hoy)
}

/// Agrega una medición de seguidores y reescribe la nota.
pub fn medir_seguidores(vault: &Vault, s: &Seguidores, hoy: &str) -> Result<PathBuf, String> {
    let reg = Registro::leer(&vault.registro())?;
    estadisticas::agregar_seguidores(&vault.seguidores_csv(), s, &reg)?;
    escribir_nota(vault, hoy)
}

/// Pide un reel: un encargo `pendiente` en el `encargos.toml` del repo de la
/// cuenta. Devuelve el encargo y el repo.
pub fn pedir(
    vault: &Vault,
    cuenta: &str,
    idea: &str,
    hoy: &str,
) -> Result<(Encargo, PathBuf), String> {
    let reg = Registro::leer(&vault.registro())?;
    let c = reg.cuenta(cuenta)?;
    let repo = reg.repo(c);
    let e = tablero_de(&repo)?.encargar(&c.nombre, idea, hoy, None)?;
    Ok((e, repo))
}

/// Marca un post publicado en el README de su repo. Devuelve la ruta del
/// README (relativa al repo), la línea nueva y el repo.
pub fn marcar_publicado(
    vault: &Vault,
    cuenta: &str,
    post: &str,
    fecha: &str,
    enlace: Option<&str>,
) -> Result<(String, String, PathBuf), String> {
    if !slug_valido(post) {
        return Err(format!("post «{post}»: el nombre de su carpeta"));
    }
    let reg = Registro::leer(&vault.registro())?;
    let c = reg.cuenta(cuenta)?;
    let repo = reg.repo(c);
    let t = tablero_de(&repo)?;
    let tc = t
        .cuenta(&c.nombre)
        .ok_or_else(|| format!("«{}» no está en su guion-cuentas.toml", c.nombre))?;
    let p = tc
        .posts
        .iter()
        .find(|p| p.nombre == post)
        .ok_or_else(|| format!("{} no tiene el post «{post}»", c.nombre))?;
    let linea = publicado::marcar(&p.dir.join("README.md"), fecha, enlace)?;
    Ok((format!("{}/README.md", p.ruta), linea, repo))
}

/// Un post de un repo, como lo necesita la interfaz.
#[derive(Debug, Clone, Serialize)]
pub struct PostRepo {
    pub nombre: String,
    pub titulo: Option<String>,
    pub estado: Option<String>,
}

/// Una cuenta con sus posts y sus pedidos.
#[derive(Debug, Clone, Serialize)]
pub struct CuentaRepo {
    pub slug: String,
    pub nombre: String,
    pub instagram: String,
    pub repo: String,
    pub posts: Vec<PostRepo>,
    pub pedidos: Vec<Encargo>,
    pub aviso: Option<String>,
}

/// Cada cuenta del registro con los posts y los pedidos de su repo. Un repo
/// que no se lee deja su aviso, no tumba a los demás.
pub fn cuentas(vault: &Vault) -> Result<Vec<CuentaRepo>, String> {
    let reg = Registro::leer(&vault.registro())?;
    let tableros = resumen::leer_tableros(&reg);
    Ok(reg
        .cuentas
        .iter()
        .map(|c| {
            let repo = reg.repo(c);
            let mut out = CuentaRepo {
                slug: c.slug.clone(),
                nombre: c.nombre.clone(),
                instagram: c.instagram.clone(),
                repo: repo.display().to_string(),
                posts: Vec::new(),
                pedidos: Vec::new(),
                aviso: None,
            };
            match tableros.iter().find(|(r, _)| *r == repo).map(|(_, t)| t) {
                Some(Ok(t)) => match t.cuenta(&c.nombre) {
                    Some(tc) => {
                        out.posts = tc
                            .posts
                            .iter()
                            .map(|p| PostRepo {
                                nombre: p.nombre.clone(),
                                titulo: p.titulo.clone(),
                                estado: p.estado.map(|e| e.nombre().to_string()),
                            })
                            .collect();
                        out.pedidos = t.encargos_de(tc).cloned().collect();
                    }
                    None => {
                        out.aviso = Some(format!("«{}» no está en su guion-cuentas.toml", c.nombre))
                    }
                },
                Some(Err(e)) => out.aviso = Some(e.clone()),
                None => {}
            }
            out
        })
        .collect())
}
