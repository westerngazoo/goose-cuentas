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

/// El `publicar/ORDEN.md` de la casa: el del primer repo del registro que lo
/// tenga. Devuelve (raíz del repo, ruta del archivo).
pub fn orden_md(reg: &Registro) -> Result<(PathBuf, PathBuf), String> {
    reg.repos()
        .into_iter()
        .map(|r| {
            let f = r.join("publicar/ORDEN.md");
            (r, f)
        })
        .find(|(_, f)| f.is_file())
        .ok_or_else(|| "ningún repo del registro tiene publicar/ORDEN.md".to_string())
}

/// Una entrada de ORDEN.md con lo que hay en su carpeta.
#[derive(Debug, Clone, Serialize)]
pub struct Paquete {
    pub titulo: String,
    pub carpeta: String,
    pub publicado: bool,
    pub fecha: Option<String>,
    pub enlace: Option<String>,
    /// Rutas relativas a la raíz del repo (para `/archivo?ruta=`).
    pub video: Option<String>,
    pub portada: Option<String>,
    /// Las mismas, absolutas en esta máquina (para copiarlas y subir el
    /// archivo desde el navegador o el Finder).
    pub video_abs: Option<String>,
    pub portada_abs: Option<String>,
    /// El de `caption.txt` (lo que se pega); si no hay, el de ORDEN.md.
    pub caption: Option<String>,
    pub alt: Option<String>,
    pub caracteres: usize,
    pub hashtags: usize,
    pub falta: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SeccionVista {
    pub titulo: String,
    pub nota: String,
    pub paquetes: Vec<Paquete>,
}

fn leer_txt(p: &Path) -> Option<String> {
    std::fs::read_to_string(p)
        .ok()
        .map(|t| t.trim_end().to_string())
}

/// Las secciones de ORDEN.md con sus paquetes, en orden.
pub fn orden(vault: &Vault) -> Result<(PathBuf, Vec<SeccionVista>), String> {
    let reg = Registro::leer(&vault.registro())?;
    let (raiz, archivo) = orden_md(&reg)?;
    let texto =
        std::fs::read_to_string(&archivo).map_err(|e| format!("{}: {e}", archivo.display()))?;
    let secciones = crate::orden::parse(&texto)
        .into_iter()
        .filter(|s| !s.entradas.is_empty())
        .map(|s| SeccionVista {
            titulo: s.titulo,
            nota: s.nota,
            paquetes: s
                .entradas
                .into_iter()
                .map(|e| {
                    let dir = raiz.join(&e.carpeta);
                    let mut nombres: Vec<String> = std::fs::read_dir(&dir)
                        .map(|it| {
                            it.flatten()
                                .filter_map(|d| d.file_name().into_string().ok())
                                .collect()
                        })
                        .unwrap_or_default();
                    nombres.sort();
                    let rel = |n: &String| format!("{}/{n}", e.carpeta);
                    let video = nombres
                        .iter()
                        .find(|n| n.ends_with(".mp4") && !n.starts_with('.'))
                        .map(rel);
                    let portada = nombres.iter().find(|n| n.ends_with("portada.png")).map(rel);
                    let abs = |r: &Option<String>| {
                        r.as_ref().map(|r| {
                            let p = raiz.join(r);
                            std::fs::canonicalize(&p).unwrap_or(p).display().to_string()
                        })
                    };
                    let (video_abs, portada_abs) = (abs(&video), abs(&portada));
                    let caption = leer_txt(&dir.join("caption.txt")).or(e.caption);
                    let alt = leer_txt(&dir.join("alt-text.txt")).or(e.alt);
                    let mut falta = Vec::new();
                    if !dir.is_dir() {
                        falta.push("la carpeta".to_string());
                    } else {
                        for (que, v) in [
                            ("el video", video.is_some()),
                            ("la portada", portada.is_some()),
                            ("el caption", caption.is_some()),
                            ("el texto alternativo", alt.is_some()),
                        ] {
                            if !v {
                                falta.push(que.to_string());
                            }
                        }
                    }
                    let (caracteres, hashtags) = caption
                        .as_deref()
                        .map(|c| {
                            (
                                c.chars().count(),
                                c.split_whitespace().filter(|w| w.starts_with('#')).count(),
                            )
                        })
                        .unwrap_or((0, 0));
                    Paquete {
                        titulo: e.titulo,
                        carpeta: e.carpeta,
                        publicado: e.publicado,
                        fecha: e.fecha,
                        enlace: e.enlace,
                        video,
                        portada,
                        video_abs,
                        portada_abs,
                        caption,
                        alt,
                        caracteres,
                        hashtags,
                        falta,
                    }
                })
                .collect(),
        })
        .collect();
    Ok((raiz, secciones))
}

/// El archivo de `ruta` (relativa al repo de ORDEN.md), si existe y está
/// dentro de `publicar/`; si no, por qué no.
pub fn archivo_de_publicar(vault: &Vault, ruta: &str) -> Result<PathBuf, String> {
    let reg = Registro::leer(&vault.registro())?;
    let (raiz, _) = orden_md(&reg)?;
    let base = std::fs::canonicalize(raiz.join("publicar")).map_err(|_| "no existe publicar/")?;
    let f = std::fs::canonicalize(raiz.join(ruta)).map_err(|_| "no existe".to_string())?;
    if !f.starts_with(&base) || !f.is_file() {
        return Err("fuera de publicar/".into());
    }
    Ok(f)
}

/// Muestra el archivo en el explorador del sistema (Finder en macOS). Solo
/// archivos de `publicar/`. Devuelve el comando que corrió.
pub fn revelar(vault: &Vault, ruta: &str) -> Result<String, String> {
    let f = archivo_de_publicar(vault, ruta)?;
    let (cmd, args): (&str, Vec<std::ffi::OsString>) = if cfg!(target_os = "macos") {
        ("open", vec!["-R".into(), f.clone().into_os_string()])
    } else if cfg!(target_os = "windows") {
        ("explorer", vec![format!("/select,{}", f.display()).into()])
    } else {
        let dir = f
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| f.clone());
        ("xdg-open", vec![dir.into_os_string()])
    };
    std::process::Command::new(cmd)
        .args(&args)
        .spawn()
        .map_err(|e| format!("{cmd}: {e}"))?;
    Ok(format!("{cmd} {}", f.display()))
}

/// Marca publicada la entrada de `carpeta` en ORDEN.md y, si se encuentra el
/// post cuyo paquete es esa carpeta, también su README. Devuelve lo que hizo.
pub fn marcar_orden(
    vault: &Vault,
    carpeta: &str,
    fecha: &str,
    enlace: Option<&str>,
) -> Result<Vec<String>, String> {
    if !guion_tablero::fecha::es_fecha(fecha) {
        return Err(format!("fecha «{fecha}»: AAAA-MM-DD"));
    }
    if let Some(e) = enlace {
        if !e.starts_with("https://") || e.contains(char::is_whitespace) {
            return Err(format!("enlace «{e}»: una URL https sin espacios"));
        }
    }
    let reg = Registro::leer(&vault.registro())?;
    let (raiz, archivo) = orden_md(&reg)?;
    let texto =
        std::fs::read_to_string(&archivo).map_err(|e| format!("{}: {e}", archivo.display()))?;
    let nuevo = crate::orden::marcar(&texto, carpeta, fecha, enlace)?;
    std::fs::write(&archivo, nuevo).map_err(|e| format!("{}: {e}", archivo.display()))?;
    let mut hecho = vec![format!("ORDEN.md: «{carpeta}» publicado")];

    // el README del post cuyo paquete es esta carpeta
    let destino = std::fs::canonicalize(raiz.join(carpeta.trim_end_matches('/'))).ok();
    for repo in reg.repos() {
        let Ok(t) = tablero_de(&repo) else { continue };
        for c in &t.cuentas {
            for p in &c.posts {
                let Some(pub_dir) = &p.publicar else { continue };
                let mismo = std::fs::canonicalize(c.raiz.join(pub_dir)).ok();
                if mismo.is_some() && mismo == destino {
                    let readme = p.dir.join("README.md");
                    match publicado::marcar(&readme, fecha, enlace) {
                        Ok(_) => hecho.push(format!("{}/README.md: publicado", p.ruta)),
                        Err(e) => hecho.push(format!("{}/README.md: {e}", p.ruta)),
                    }
                }
            }
        }
    }
    hecho.push(format!("falta el commit en {}", raiz.display()));
    Ok(hecho)
}
