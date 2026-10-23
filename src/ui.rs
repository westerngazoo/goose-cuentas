//! `cuentas ui`: la interfaz local. Un servidor HTTP/1.1 hecho a mano, solo
//! std, una petición a la vez, atado a 127.0.0.1: nadie fuera de esta máquina
//! lo ve.
//!
//! | Petición | Respuesta |
//! |---|---|
//! | `GET /` | la página (va dentro del binario) |
//! | `GET /api/estado` | el resumen, las mediciones, las cuentas con sus posts y pedidos |
//! | `POST /api/medicion` | una medición de un post → `{"ok"}` o 400 con el problema |
//! | `POST /api/seguidores` | una medición de seguidores |
//! | `POST /api/pedido` | `{"cuenta","idea"}` y, si quieres, `formato`, `prioridad`, `para`, `nivel`, `variantes`, `remake`, `notas` → el encargo |
//! | `POST /api/publicado` | `{"cuenta","post","enlace"?,"fecha"?}` → la línea nueva |
//! | `GET /api/orden` | las secciones de `publicar/ORDEN.md` con sus paquetes |
//! | `POST /api/orden/publicado` | `{"carpeta","enlace"?,"fecha"?}` → ORDEN.md y el README del post |
//! | `GET /avatar?cuenta=<slug>` | el avatar que declara el repo de la cuenta (png, jpg, webp o svg, dentro del repo) |
//! | `GET /archivo?ruta=publicar/…` | un MP4, PNG o TXT del paquete (con `Range`; `&descargar=1` lo baja) |
//! | `POST /api/revelar` | `{"ruta"}` → muestra el archivo en el Finder (solo de `publicar/`) |
//!
//! Dos defensas baratas contra una página ajena que le hable a localhost: el
//! `Host` tiene que ser `127.0.0.1:<puerto>` o `localhost:<puerto>` (contra el
//! DNS rebinding), y todo POST viaja como `application/json` (un formulario
//! ajeno no puede mandarlo sin una consulta CORS previa, que aquí no se
//! contesta). `/archivo` solo sirve MP4, PNG y TXT de dentro de `publicar/`.
//! [`responder`] es todo el enrutador y no toca sockets, así que se prueba sin
//! red. Cada conexión va en su hilo (un video no frena al resto) y las
//! escrituras pasan de una en una.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::{Arc, Mutex};

use serde::Deserialize;
use serde_json::{json, Value};

use crate::acciones;
use crate::estadisticas::{self, Medicion, Seguidores};
use crate::vault::Vault;

/// El puerto por omisión.
pub const PUERTO: u16 = 7171;

/// La página.
pub const PAGINA: &str = include_str!("ui.html");

/// Lo que el enrutador necesita de una petición.
#[derive(Debug, Clone, PartialEq)]
pub struct Peticion {
    pub metodo: String,
    pub ruta: String,
    pub query: Vec<(String, String)>,
    pub rango: Option<String>,
    pub host: String,
    pub tipo: String,
    pub cuerpo: Vec<u8>,
}

/// Una respuesta.
#[derive(Debug, Clone, PartialEq)]
pub struct Respuesta {
    pub estado: u16,
    pub tipo: &'static str,
    pub cuerpo: Vec<u8>,
    pub extra: Vec<(&'static str, String)>,
}

impl Respuesta {
    fn json(estado: u16, v: &Value) -> Respuesta {
        Respuesta {
            estado,
            tipo: "application/json; charset=utf-8",
            cuerpo: v.to_string().into_bytes(),
            extra: Vec::new(),
        }
    }

    fn error(estado: u16, m: &str) -> Respuesta {
        Respuesta::json(estado, &json!({ "error": m }))
    }

    pub fn texto(&self) -> String {
        String::from_utf8_lossy(&self.cuerpo).into_owned()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MedicionIn {
    cuenta: String,
    post: String,
    #[serde(default)]
    fecha: Option<String>,
    #[serde(default)]
    vistas: Option<u64>,
    #[serde(default)]
    alcance: Option<u64>,
    #[serde(default)]
    likes: Option<u64>,
    #[serde(default)]
    comentarios: Option<u64>,
    #[serde(default)]
    guardados: Option<u64>,
    #[serde(default)]
    compartidos: Option<u64>,
    #[serde(default)]
    seguidores_ganados: Option<u64>,
    #[serde(default)]
    nota: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SeguidoresIn {
    cuenta: String,
    seguidores: u64,
    #[serde(default)]
    fecha: Option<String>,
    #[serde(default)]
    nota: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PedidoIn {
    cuenta: String,
    idea: String,
    #[serde(default)]
    formato: Option<String>,
    #[serde(default)]
    prioridad: Option<String>,
    #[serde(default)]
    para: Option<String>,
    #[serde(default)]
    nivel: Option<String>,
    #[serde(default)]
    variantes: Vec<String>,
    #[serde(default)]
    remake: Option<String>,
    #[serde(default)]
    notas: Option<String>,
}

impl PedidoIn {
    fn detalles(&self) -> guion_tablero::encargos::Detalles {
        guion_tablero::encargos::Detalles {
            formato: self.formato.clone(),
            prioridad: self.prioridad.clone(),
            para: self.para.clone(),
            nivel: self.nivel.clone(),
            variantes: self.variantes.clone(),
            remake: self.remake.clone(),
            notas: self.notas.clone(),
            post: None,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicadoIn {
    cuenta: String,
    post: String,
    #[serde(default)]
    enlace: Option<String>,
    #[serde(default)]
    fecha: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RevelarIn {
    ruta: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OrdenIn {
    carpeta: String,
    #[serde(default)]
    enlace: Option<String>,
    #[serde(default)]
    fecha: Option<String>,
}

fn leer<T: for<'de> Deserialize<'de>>(p: &Peticion) -> Result<T, Respuesta> {
    serde_json::from_slice(&p.cuerpo).map_err(|e| Respuesta::error(400, &e.to_string()))
}

fn vacio(s: Option<String>) -> Option<String> {
    s.filter(|x| !x.trim().is_empty())
}

/// El enrutador.
pub fn responder(vault: &Vault, puerto: u16, hoy: &str, p: &Peticion) -> Respuesta {
    let host_ok = [format!("127.0.0.1:{puerto}"), format!("localhost:{puerto}")]
        .iter()
        .any(|h| h.eq_ignore_ascii_case(&p.host));
    if !host_ok {
        return Respuesta::error(403, "solo desde esta máquina");
    }
    if p.metodo == "POST" && !p.tipo.starts_with("application/json") {
        return Respuesta::error(415, "los POST van como application/json");
    }
    let r = match (p.metodo.as_str(), p.ruta.as_str()) {
        ("GET", "/") => {
            return Respuesta {
                estado: 200,
                tipo: "text/html; charset=utf-8",
                cuerpo: PAGINA.as_bytes().to_vec(),
                extra: Vec::new(),
            }
        }
        ("GET", "/api/estado") => estado(vault, hoy),
        ("GET", "/api/orden") => acciones::orden(vault).map(|(raiz, secciones)| {
            json!({ "raiz": raiz.display().to_string(), "hoy": hoy, "secciones": secciones })
        }),
        ("GET", "/archivo") => return archivo(vault, p),
        ("GET", "/avatar") => return avatar(vault, p),
        ("POST", "/api/revelar") => match leer::<RevelarIn>(p) {
            Ok(x) => acciones::revelar(vault, &x.ruta).map(|c| json!({ "ok": c })),
            Err(r) => return r,
        },
        ("POST", "/api/orden/publicado") => match leer::<OrdenIn>(p) {
            Ok(x) => {
                let fecha = vacio(x.fecha).unwrap_or_else(|| hoy.to_string());
                let enlace = vacio(x.enlace);
                acciones::marcar_orden(vault, &x.carpeta, &fecha, enlace.as_deref())
                    .map(|hecho| json!({ "ok": hecho.join(" · ") }))
            }
            Err(r) => return r,
        },
        ("POST", "/api/medicion") => match leer::<MedicionIn>(p) {
            Ok(m) => {
                let m = Medicion {
                    fecha: vacio(m.fecha).unwrap_or_else(|| hoy.to_string()),
                    cuenta: m.cuenta,
                    post: m.post.trim().to_string(),
                    vistas: m.vistas,
                    alcance: m.alcance,
                    likes: m.likes,
                    comentarios: m.comentarios,
                    guardados: m.guardados,
                    compartidos: m.compartidos,
                    seguidores_ganados: m.seguidores_ganados,
                    nota: m.nota.trim().to_string(),
                };
                acciones::medir_post(vault, &m, hoy).map(|_| {
                    json!({ "ok": format!("{} · {} del {}", m.cuenta, m.post, m.fecha) })
                })
            }
            Err(r) => return r,
        },
        ("POST", "/api/seguidores") => match leer::<SeguidoresIn>(p) {
            Ok(s) => {
                let s = Seguidores {
                    fecha: vacio(s.fecha).unwrap_or_else(|| hoy.to_string()),
                    cuenta: s.cuenta,
                    seguidores: s.seguidores,
                    nota: s.nota.trim().to_string(),
                };
                acciones::medir_seguidores(vault, &s, hoy).map(|_| {
                    json!({ "ok": format!("{}: {} seguidores el {}", s.cuenta, s.seguidores, s.fecha) })
                })
            }
            Err(r) => return r,
        },
        ("POST", "/api/pedido") => match leer::<PedidoIn>(p) {
            Ok(x) => acciones::pedir(vault, &x.cuenta, &x.idea, hoy, &x.detalles()).map(|(e, repo)| {
                json!({ "ok": format!("{} pedido: «{}»", e.id, e.idea), "encargo": e, "repo": repo.display().to_string() })
            }),
            Err(r) => return r,
        },
        ("POST", "/api/publicado") => match leer::<PublicadoIn>(p) {
            Ok(x) => {
                let fecha = vacio(x.fecha).unwrap_or_else(|| hoy.to_string());
                let enlace = vacio(x.enlace);
                acciones::marcar_publicado(vault, &x.cuenta, &x.post, &fecha, enlace.as_deref())
                    .map(|(readme, linea, repo)| {
                        json!({ "ok": format!("{readme}: {linea}"), "repo": repo.display().to_string() })
                    })
            }
            Err(r) => return r,
        },
        (
            _,
            "/" | "/api/estado" | "/api/medicion" | "/api/seguidores" | "/api/pedido"
            | "/api/publicado" | "/api/orden" | "/api/orden/publicado" | "/archivo"
            | "/avatar" | "/api/revelar",
        ) => {
            return Respuesta::error(405, "método no permitido")
        }
        _ => return Respuesta::error(404, "no existe"),
    };
    match r {
        Ok(v) => Respuesta::json(200, &v),
        Err(e) => Respuesta::error(400, &e),
    }
}

fn estado(vault: &Vault, hoy: &str) -> Result<Value, String> {
    let resumen = acciones::resumen(vault, hoy)?;
    let cuentas = acciones::cuentas(vault)?;
    let mediciones = estadisticas::leer_posts(&vault.posts_csv())?;
    let seguidores = estadisticas::leer_seguidores(&vault.seguidores_csv())?;
    Ok(json!({
        "hoy": hoy,
        "vault": vault.cuentas().display().to_string(),
        "resumen": resumen,
        "cuentas": cuentas,
        "mediciones": mediciones,
        "seguidores": seguidores,
    }))
}

fn param<'a>(p: &'a Peticion, k: &str) -> Option<&'a str> {
    p.query
        .iter()
        .find(|(n, _)| n == k)
        .map(|(_, v)| v.as_str())
}

/// El avatar de una cuenta (`?cuenta=<slug>`), el que declara su repo.
fn avatar(vault: &Vault, p: &Peticion) -> Respuesta {
    let Some(slug) = param(p, "cuenta") else {
        return Respuesta::error(400, "falta ?cuenta=");
    };
    match acciones::avatar(vault, slug).and_then(|(ruta, tipo)| {
        std::fs::read(&ruta)
            .map(|b| (b, tipo))
            .map_err(|e| format!("{}: {e}", ruta.display()))
    }) {
        Ok((cuerpo, tipo)) => Respuesta {
            estado: 200,
            tipo,
            cuerpo,
            extra: vec![
                ("Cache-Control", "max-age=300".to_string()),
                // un SVG no corre scripts ni carga nada fuera de sí
                (
                    "Content-Security-Policy",
                    "default-src 'none'; style-src 'unsafe-inline'".to_string(),
                ),
                ("X-Content-Type-Options", "nosniff".to_string()),
            ],
        },
        Err(e) => Respuesta::error(404, &e),
    }
}

/// Un archivo de un paquete de `publicar/`: solo MP4, PNG o TXT, y solo de
/// dentro de esa carpeta (ni `..` ni enlaces que salgan de ella).
fn archivo(vault: &Vault, p: &Peticion) -> Respuesta {
    let Some(ruta) = param(p, "ruta") else {
        return Respuesta::error(400, "falta ?ruta=");
    };
    let tipo = match Path::new(ruta).extension().and_then(|e| e.to_str()) {
        Some("mp4") => "video/mp4",
        Some("png") => "image/png",
        Some("txt") => "text/plain; charset=utf-8",
        _ => return Respuesta::error(403, "solo MP4, PNG o TXT"),
    };
    let reg = match crate::registro::Registro::leer(&vault.registro()) {
        Ok(r) => r,
        Err(e) => return Respuesta::error(400, &e),
    };
    let f = match acciones::ruta_en_publicar(&reg, ruta) {
        Ok(f) => f,
        Err((estado, e)) => return Respuesta::error(estado, &e),
    };
    let datos = match std::fs::read(&f) {
        Ok(d) => d,
        Err(e) => return Respuesta::error(404, &e.to_string()),
    };
    let total = datos.len();
    let mut extra = vec![("Accept-Ranges", "bytes".to_string())];
    if param(p, "descargar").is_some() {
        let nombre = f
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("archivo")
            .replace('"', "");
        extra.push((
            "Content-Disposition",
            format!("attachment; filename=\"{nombre}\""),
        ));
    }
    if let Some((a, b)) = p.rango.as_deref().and_then(|r| rango(r, total)) {
        extra.push(("Content-Range", format!("bytes {a}-{b}/{total}")));
        return Respuesta {
            estado: 206,
            tipo,
            cuerpo: datos[a..=b].to_vec(),
            extra,
        };
    }
    Respuesta {
        estado: 200,
        tipo,
        cuerpo: datos,
        extra,
    }
}

/// `bytes=a-b`, `bytes=a-` o `bytes=-n` dentro de `total` bytes.
fn rango(r: &str, total: usize) -> Option<(usize, usize)> {
    let (a, b) = r
        .strip_prefix("bytes=")?
        .split(',')
        .next()?
        .split_once('-')?;
    let (a, b) = (a.trim(), b.trim());
    if total == 0 {
        return None;
    }
    let (ini, fin) = if a.is_empty() {
        let n: usize = b.parse().ok()?;
        (total.saturating_sub(n), total - 1)
    } else {
        let ini: usize = a.parse().ok()?;
        let fin = if b.is_empty() {
            total - 1
        } else {
            b.parse::<usize>().ok()?.min(total - 1)
        };
        (ini, fin)
    };
    (ini <= fin && ini < total).then_some((ini, fin))
}

/// `%XX` y `+` decodificados.
fn decodificar(s: &str) -> String {
    let hex = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match (
            b[i],
            b.get(i + 1).copied().and_then(hex),
            b.get(i + 2).copied().and_then(hex),
        ) {
            (b'+', _, _) => out.push(b' '),
            (b'%', Some(h), Some(l)) => {
                out.push(h * 16 + l);
                i += 2;
            }
            (c, _, _) => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn query(q: &str) -> Vec<(String, String)> {
    q.split('&')
        .filter(|s| !s.is_empty())
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            (decodificar(k), decodificar(v))
        })
        .collect()
}

const MAX_CUERPO: usize = 64 * 1024;

fn leer_peticion(s: &TcpStream) -> Result<Peticion, String> {
    let mut r = BufReader::new(s);
    let mut linea = String::new();
    r.read_line(&mut linea).map_err(|e| e.to_string())?;
    let mut partes = linea.split_whitespace();
    let metodo = partes.next().ok_or("petición vacía")?.to_string();
    let objetivo = partes.next().ok_or("sin ruta")?;
    let (ruta, q) = objetivo.split_once('?').unwrap_or((objetivo, ""));
    let (ruta, query) = (decodificar(ruta), query(q));
    let (mut host, mut tipo, mut largo) = (String::new(), String::new(), 0usize);
    let mut rango = None;
    loop {
        let mut h = String::new();
        if r.read_line(&mut h).map_err(|e| e.to_string())? == 0 {
            break;
        }
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            let v = v.trim();
            match k.trim().to_ascii_lowercase().as_str() {
                "host" => host = v.to_string(),
                "content-type" => tipo = v.to_ascii_lowercase(),
                "content-length" => largo = v.parse().map_err(|_| "content-length")?,
                "range" => rango = Some(v.to_string()),
                _ => {}
            }
        }
    }
    if largo > MAX_CUERPO {
        return Err("cuerpo demasiado grande".into());
    }
    let mut cuerpo = vec![0; largo];
    r.read_exact(&mut cuerpo).map_err(|e| e.to_string())?;
    Ok(Peticion {
        metodo,
        ruta,
        query,
        rango,
        host,
        tipo,
        cuerpo,
    })
}

fn escribir(mut s: &TcpStream, r: &Respuesta) -> std::io::Result<()> {
    let razon = match r.estado {
        200 => "OK",
        206 => "Partial Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        415 => "Unsupported Media Type",
        _ => "Error",
    };
    write!(
        s,
        "HTTP/1.1 {} {razon}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n",
        r.estado,
        r.tipo,
        r.cuerpo.len()
    )?;
    for (k, v) in &r.extra {
        write!(s, "{k}: {v}\r\n")?;
    }
    write!(s, "\r\n")?;
    s.write_all(&r.cuerpo)?;
    s.flush()
}

/// Las escrituras (los POST) van de una en una, aunque cada conexión tenga
/// su hilo.
static ESCRIBIR: Mutex<()> = Mutex::new(());

fn atender(vault: &Vault, puerto: u16, s: &TcpStream) {
    let r = match leer_peticion(s) {
        Ok(p) if p.metodo == "POST" => {
            let _turno = ESCRIBIR.lock().unwrap_or_else(|e| e.into_inner());
            responder(vault, puerto, &guion_tablero::fecha::hoy(), &p)
        }
        Ok(p) => responder(vault, puerto, &guion_tablero::fecha::hoy(), &p),
        Err(e) => Respuesta::error(400, &e),
    };
    if let Err(e) = escribir(s, &r) {
        eprintln!("cuentas ui: {e}");
    }
}

/// Sirve hasta que se cierre el proceso.
pub fn servir(vault: &Vault, puerto: u16) -> Result<(), String> {
    let l = TcpListener::bind(("127.0.0.1", puerto)).map_err(|e| {
        format!("127.0.0.1:{puerto}: {e} (¿otro `cuentas ui` abierto? usa --puerto)")
    })?;
    println!("cuentas ui: http://127.0.0.1:{puerto}  (Ctrl-C para cerrar)");
    println!("vault: {}", vault.cuentas().display());
    let vault = Arc::new(vault.clone());
    for s in l.incoming().flatten() {
        let v = Arc::clone(&vault);
        std::thread::spawn(move || atender(&v, puerto, &s));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn pet(metodo: &str, ruta: &str, cuerpo: &str) -> Peticion {
        let (ruta, q) = ruta.split_once('?').unwrap_or((ruta, ""));
        Peticion {
            metodo: metodo.into(),
            ruta: ruta.into(),
            query: query(q),
            rango: None,
            host: "127.0.0.1:7171".into(),
            tipo: "application/json".into(),
            cuerpo: cuerpo.as_bytes().to_vec(),
        }
    }

    /// Un vault y un repo de cuenta de mentira, en una carpeta temporal.
    fn mundo(nombre: &str) -> (Vault, PathBuf) {
        let d =
            std::env::temp_dir().join(format!("goose-cuentas-ui-{nombre}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let repo = d.join("repo");
        std::fs::create_dir_all(repo.join("posts/deriva")).unwrap();
        std::fs::write(
            repo.join("guion-cuentas.toml"),
            "[[cuenta]]\nnombre = \"Rotor\"\nposts = [\"posts/*\"]\n",
        )
        .unwrap();
        std::fs::write(
            repo.join("posts/deriva/README.md"),
            "# ¿Qué es la deriva?\n\n- **estado:** listo (2026-10-16)\n",
        )
        .unwrap();
        let vault = Vault {
            raiz: d.join("vault"),
            carpeta: "cuentas".into(),
        };
        std::fs::create_dir_all(&vault.raiz).unwrap();
        vault.iniciar().unwrap();
        std::fs::write(
            vault.registro(),
            format!(
                "version = 1\n[[cuenta]]\nslug = \"rotor\"\nnombre = \"Rotor\"\ninstagram = \"rotorfisicoracing\"\nrepo = {:?}\n",
                repo.display().to_string()
            ),
        )
        .unwrap();
        (vault, d)
    }

    #[test]
    fn defensas() {
        let v = Vault {
            raiz: Path::new("/nada").into(),
            carpeta: "cuentas".into(),
        };
        let mut p = pet("GET", "/", "");
        p.host = "evil.example:7171".into();
        assert_eq!(responder(&v, 7171, "2026-10-20", &p).estado, 403);
        let mut p = pet("POST", "/api/pedido", "{}");
        p.tipo = "text/plain".into();
        assert_eq!(responder(&v, 7171, "2026-10-20", &p).estado, 415);
        assert_eq!(
            responder(&v, 7171, "2026-10-20", &pet("GET", "/x", "")).estado,
            404
        );
        assert_eq!(
            responder(&v, 7171, "2026-10-20", &pet("DELETE", "/api/estado", "")).estado,
            405
        );
        let r = responder(&v, 7171, "2026-10-20", &pet("GET", "/", ""));
        assert_eq!(r.estado, 200);
        assert!(r.texto().contains("<title>"));
    }

    #[test]
    fn capturar_pedir_y_publicar() {
        let (v, d) = mundo("todo");
        let hoy = "2026-10-20";
        let r = responder(
            &v,
            7171,
            hoy,
            &pet(
                "POST",
                "/api/seguidores",
                r#"{"cuenta":"rotor","seguidores":390}"#,
            ),
        );
        assert_eq!(r.estado, 200, "{}", r.texto());
        let r = responder(
            &v,
            7171,
            hoy,
            &pet(
                "POST",
                "/api/medicion",
                r#"{"cuenta":"rotor","post":"deriva","vistas":1500,"likes":90,"fecha":""}"#,
            ),
        );
        assert_eq!(r.estado, 200, "{}", r.texto());
        let r = responder(
            &v,
            7171,
            hoy,
            &pet(
                "POST",
                "/api/medicion",
                r#"{"cuenta":"rotor","post":"deriva"}"#,
            ),
        );
        assert_eq!(r.estado, 400);
        assert!(r.texto().contains("ninguna cifra"));
        let r = responder(
            &v,
            7171,
            hoy,
            &pet(
                "POST",
                "/api/medicion",
                r#"{"cuenta":"rotor","post":"deriva","vistas":-3}"#,
            ),
        );
        assert_eq!(r.estado, 400);

        let r = responder(
            &v,
            7171,
            hoy,
            &pet(
                "POST",
                "/api/pedido",
                r#"{"cuenta":"rotor","idea":"¿Por qué derrapa?"}"#,
            ),
        );
        assert_eq!(r.estado, 200, "{}", r.texto());
        assert!(r.texto().contains("E-0001"));

        let r = responder(
            &v,
            7171,
            hoy,
            &pet(
                "POST",
                "/api/publicado",
                r#"{"cuenta":"rotor","post":"deriva","enlace":"https://www.instagram.com/reel/abc/"}"#,
            ),
        );
        assert_eq!(r.estado, 200, "{}", r.texto());

        let r = responder(&v, 7171, hoy, &pet("GET", "/api/estado", ""));
        assert_eq!(r.estado, 200, "{}", r.texto());
        let e: Value = serde_json::from_slice(&r.cuerpo).unwrap();
        let c = &e["resumen"]["cuentas"][0];
        assert_eq!(c["seguidores"], 390);
        assert_eq!(c["vistas"], 1500);
        assert_eq!(e["cuentas"][0]["posts"][0]["estado"], "publicado");
        assert_eq!(e["cuentas"][0]["pedidos"][0]["idea"], "¿Por qué derrapa?");
        assert!(std::fs::read_to_string(v.resumen_md())
            .unwrap()
            .contains("| Rotor | 390 |"));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn un_pedido_con_detalles_y_su_estado_vivo() {
        let (v, d) = mundo("pedido");
        let hoy = "2026-10-22";
        let pedir = |cuerpo: &str| responder(&v, 7171, hoy, &pet("POST", "/api/pedido", cuerpo));
        let r = pedir(
            r#"{"cuenta":"rotor","idea":"¿Pegado o inclinado?","formato":"short","prioridad":"alta","para":"2026-10-30","nivel":"1.3","variantes":["pegado","inclinado"],"remake":"31-triceps","notas":"sin músculos"}"#,
        );
        assert_eq!(r.estado, 200, "{}", r.texto());
        // lo que no se entiende se rechaza, y no se escribe nada
        for (malo, dice) in [
            (r#"{"cuenta":"rotor","idea":"x","urgente":true}"#, "urgente"),
            (
                r#"{"cuenta":"rotor","idea":"x","formato":"tiktok"}"#,
                "formato",
            ),
            (
                r#"{"cuenta":"rotor","idea":"x","variantes":["solo"]}"#,
                "variantes",
            ),
            (r#"{"cuenta":"rotor","idea":"x","para":"30/10"}"#, "para"),
        ] {
            let r = pedir(malo);
            assert_eq!(r.estado, 400, "{malo}");
            assert!(r.texto().contains(dice), "{malo}: {}", r.texto());
        }
        let repo = d.join("repo");
        let todos = guion_tablero::encargos::load(&repo).unwrap();
        assert_eq!(todos.len(), 1);
        let e = &todos[0];
        assert_eq!(
            (
                e.formato.as_deref(),
                e.prioridad.as_deref(),
                e.remake.as_deref()
            ),
            (Some("short"), Some("alta"), Some("31-triceps"))
        );
        assert_eq!(e.variantes, ["pegado", "inclinado"]);
        // el agente lo toma para «deriva» (listo): la interfaz lo ve listo, con su post
        guion_tablero::encargos::tomar(&repo, "E-0001", "deriva").unwrap();
        let r = responder(&v, 7171, hoy, &pet("GET", "/api/estado", ""));
        let j: Value = serde_json::from_slice(&r.cuerpo).unwrap();
        let p = &j["cuentas"][0]["pedidos"][0];
        assert_eq!(p["estado"], "en curso", "lo guardado");
        assert_eq!(
            p["vivo"], "listo",
            "lo que es: el estado del README de su post"
        );
        assert_eq!(p["post_ruta"], "posts/deriva");
        assert_eq!(p["variantes"][1], "inclinado");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn el_avatar_de_una_cuenta() {
        let (v, d) = mundo("avatar");
        let repo = d.join("repo");
        let hoy = "2026-10-22";
        let pedir = |q: &str| responder(&v, 7171, hoy, &pet("GET", q, ""));
        // sin declararlo: 404 con el porqué, y el estado dice que no hay
        let r = pedir("/avatar?cuenta=rotor");
        assert_eq!(r.estado, 404);
        assert!(r.texto().contains("no declara avatar"), "{}", r.texto());
        std::fs::create_dir_all(repo.join("marca")).unwrap();
        std::fs::write(repo.join("marca/av.png"), b"\x89PNG").unwrap();
        std::fs::write(
            repo.join("guion-cuentas.toml"),
            "[[cuenta]]\nnombre = \"Rotor\"\nposts = [\"posts/*\"]\navatar = \"marca/av.png\"\ncolor = \"#e0322a\"\n",
        )
        .unwrap();
        let r = pedir("/avatar?cuenta=rotor");
        assert_eq!((r.estado, r.tipo), (200, "image/png"));
        assert_eq!(r.cuerpo, b"\x89PNG");
        assert!(r.extra.iter().any(|(k, _)| *k == "Content-Security-Policy"));
        let e: Value = serde_json::from_slice(&pedir("/api/estado").cuerpo).unwrap();
        assert_eq!(
            (&e["cuentas"][0]["avatar"], &e["cuentas"][0]["color"]),
            (&json!(true), &json!("#e0322a"))
        );
        // una cuenta que no existe, o sin ?cuenta=
        assert_eq!(pedir("/avatar?cuenta=nadie").estado, 404);
        assert_eq!(pedir("/avatar").estado, 400);
        // un enlace que sale del repo no se sirve
        #[cfg(unix)]
        {
            std::fs::write(d.join("secreto.png"), b"no").unwrap();
            std::fs::remove_file(repo.join("marca/av.png")).unwrap();
            std::os::unix::fs::symlink(d.join("secreto.png"), repo.join("marca/av.png")).unwrap();
            let r = pedir("/avatar?cuenta=rotor");
            assert_eq!(r.estado, 404, "{}", r.texto());
            assert!(r.texto().contains("sale de su repo"));
        }
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn rangos() {
        assert_eq!(rango("bytes=0-9", 100), Some((0, 9)));
        assert_eq!(rango("bytes=90-", 100), Some((90, 99)));
        assert_eq!(rango("bytes=-10", 100), Some((90, 99)));
        assert_eq!(rango("bytes=50-500", 100), Some((50, 99)));
        assert_eq!(rango("bytes=200-", 100), None);
        assert_eq!(rango("items=0-1", 100), None);
    }

    #[test]
    fn el_orden_sus_archivos_y_marcar() {
        let (v, d) = mundo("orden");
        let repo = d.join("repo");
        let pk = repo.join("publicar/rotor/deriva");
        std::fs::create_dir_all(&pk).unwrap();
        std::fs::write(pk.join("deriva.mp4"), (0u8..=255).collect::<Vec<u8>>()).unwrap();
        std::fs::write(pk.join("deriva-portada.png"), b"png").unwrap();
        std::fs::write(pk.join("caption.txt"), "¿Deriva?\n\n#karting #f1\n").unwrap();
        std::fs::write(pk.join("alt-text.txt"), "Un kart.\n").unwrap();
        std::fs::write(repo.join("secreto.txt"), "no").unwrap();
        std::fs::write(
            repo.join("publicar/ORDEN.md"),
            "## @rotorfisicoracing\n\n### 1. ¿Deriva?\n\n- **Carpeta:** `publicar/rotor/deriva/`\n\n- [ ] Publicado · fecha: ______ · enlace: ______\n",
        )
        .unwrap();
        // el README del post apunta su paquete a esa carpeta
        std::fs::write(
            repo.join("guion-cuentas.toml"),
            "[[cuenta]]\nnombre = \"Rotor\"\nposts = [\"posts/*\"]\npublicar = [\"publicar/rotor\"]\n",
        )
        .unwrap();
        let hoy = "2026-10-21";
        let r = responder(&v, 7171, hoy, &pet("GET", "/api/orden", ""));
        assert_eq!(r.estado, 200, "{}", r.texto());
        let e: Value = serde_json::from_slice(&r.cuerpo).unwrap();
        let p0 = &e["secciones"][0]["paquetes"][0];
        assert_eq!(p0["titulo"], "¿Deriva?");
        assert_eq!(p0["video"], "publicar/rotor/deriva/deriva.mp4");
        assert_eq!(p0["caption"], "¿Deriva?\n\n#karting #f1");
        assert_eq!(p0["hashtags"], 2);
        assert_eq!(p0["falta"].as_array().unwrap().len(), 0);

        let mut g = pet(
            "GET",
            "/archivo?ruta=publicar%2Frotor%2Fderiva%2Fderiva.mp4",
            "",
        );
        let r = responder(&v, 7171, hoy, &g);
        assert_eq!((r.estado, r.cuerpo.len(), r.tipo), (200, 256, "video/mp4"));
        g.rango = Some("bytes=10-19".into());
        let r = responder(&v, 7171, hoy, &g);
        assert_eq!(r.estado, 206);
        assert_eq!(r.cuerpo, (10u8..20).collect::<Vec<u8>>());
        for malo in [
            "publicar/../secreto.txt",
            "guion-cuentas.toml",
            "publicar/ORDEN.md",
        ] {
            let r = responder(
                &v,
                7171,
                hoy,
                &pet("GET", &format!("/archivo?ruta={malo}"), ""),
            );
            assert_eq!(r.estado, 403, "{malo}");
        }

        let r = responder(
            &v,
            7171,
            hoy,
            &pet(
                "POST",
                "/api/orden/publicado",
                r#"{"carpeta":"publicar/rotor/deriva","enlace":"https://www.instagram.com/reel/abc/"}"#,
            ),
        );
        assert_eq!(r.estado, 200, "{}", r.texto());
        assert!(
            r.texto().contains("posts/deriva/README.md: publicado"),
            "{}",
            r.texto()
        );
        let orden = std::fs::read_to_string(repo.join("publicar/ORDEN.md")).unwrap();
        assert!(orden.contains(
            "- [x] Publicado · fecha: 2026-10-21 · enlace: https://www.instagram.com/reel/abc/"
        ));
        let readme = std::fs::read_to_string(repo.join("posts/deriva/README.md")).unwrap();
        assert!(readme.contains("publicado (2026-10-21, https://www.instagram.com/reel/abc/)"));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn cada_repo_con_su_orden() {
        // dos repos en el registro, cada uno con su publicar/ORDEN.md: la
        // pestaña Publicar los junta, y cada archivo y cada marca van al suyo
        let (v, d) = mundo("dos-ordenes");
        let uno = d.join("repo");
        let pk = uno.join("publicar/rotor/deriva");
        std::fs::create_dir_all(&pk).unwrap();
        std::fs::write(pk.join("deriva.mp4"), b"uno").unwrap();
        std::fs::write(
            uno.join("publicar/ORDEN.md"),
            "## @rotorfisicoracing\n\n### 1. ¿Deriva?\n\n- **Carpeta:** `publicar/rotor/deriva/`\n\n- [ ] Publicado · fecha: ______ · enlace: ______\n",
        )
        .unwrap();
        let dos = d.join("otro");
        let pk = dos.join("publicar/otra/mapa");
        std::fs::create_dir_all(&pk).unwrap();
        std::fs::create_dir_all(dos.join("posts")).unwrap();
        std::fs::write(pk.join("mapa.mp4"), b"dos").unwrap();
        std::fs::write(dos.join("secreto.txt"), "no").unwrap();
        std::fs::write(
            dos.join("guion-cuentas.toml"),
            "[[cuenta]]\nnombre = \"Otra\"\nposts = [\"posts/*\"]\npublicar = [\"publicar/otra\"]\n",
        )
        .unwrap();
        std::fs::write(
            dos.join("publicar/ORDEN.md"),
            "## Otra\n\n### 1. ¿El mapa?\n\n- **Carpeta:** `publicar/otra/mapa/`\n\n- [ ] Publicado · fecha: ______ · enlace: ______\n",
        )
        .unwrap();
        let mut reg = std::fs::read_to_string(v.registro()).unwrap();
        reg.push_str(&format!(
            "[[cuenta]]\nslug = \"otra\"\nnombre = \"Otra\"\ninstagram = \"\"\nrepo = {:?}\n",
            dos.display().to_string()
        ));
        std::fs::write(v.registro(), reg).unwrap();
        let hoy = "2026-10-22";

        let r = responder(&v, 7171, hoy, &pet("GET", "/api/orden", ""));
        assert_eq!(r.estado, 200, "{}", r.texto());
        let e: Value = serde_json::from_slice(&r.cuerpo).unwrap();
        let titulos: Vec<&str> = e["secciones"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["titulo"].as_str().unwrap())
            .collect();
        assert_eq!(titulos, ["@rotorfisicoracing", "Otra"]);
        assert_eq!(e["secciones"][1]["cuenta"], "otra");
        assert_eq!(
            e["secciones"][1]["paquetes"][0]["video"],
            "publicar/otra/mapa/mapa.mp4"
        );

        // cada archivo sale de su repo
        for (ruta, cuerpo) in [
            ("publicar%2Frotor%2Fderiva%2Fderiva.mp4", "uno"),
            ("publicar%2Fotra%2Fmapa%2Fmapa.mp4", "dos"),
        ] {
            let r = responder(
                &v,
                7171,
                hoy,
                &pet("GET", &format!("/archivo?ruta={ruta}"), ""),
            );
            assert_eq!((r.estado, r.texto()), (200, cuerpo.to_string()), "{ruta}");
        }
        // y del segundo repo tampoco se sale de publicar/
        let r = responder(
            &v,
            7171,
            hoy,
            &pet("GET", "/archivo?ruta=publicar/../secreto.txt", ""),
        );
        assert_eq!(r.estado, 403);

        // la marca va al ORDEN.md que tiene la carpeta, y solo a ese
        let r = responder(
            &v,
            7171,
            hoy,
            &pet(
                "POST",
                "/api/orden/publicado",
                r#"{"carpeta":"publicar/otra/mapa","enlace":"https://www.instagram.com/reel/xyz/"}"#,
            ),
        );
        assert_eq!(r.estado, 200, "{}", r.texto());
        let otro = std::fs::read_to_string(dos.join("publicar/ORDEN.md")).unwrap();
        assert!(
            otro.contains("- [x] Publicado · fecha: 2026-10-22"),
            "{otro}"
        );
        let primero = std::fs::read_to_string(uno.join("publicar/ORDEN.md")).unwrap();
        assert!(primero.contains("- [ ] Publicado"), "{primero}");

        // una carpeta que no está en ningún ORDEN.md es un error
        let r = responder(
            &v,
            7171,
            hoy,
            &pet(
                "POST",
                "/api/orden/publicado",
                r#"{"carpeta":"publicar/nada"}"#,
            ),
        );
        assert_eq!(r.estado, 400, "{}", r.texto());
        let _ = std::fs::remove_dir_all(d);
    }
}
