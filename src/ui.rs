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
//! | `POST /api/pedido` | `{"cuenta","idea"}` → el encargo |
//! | `POST /api/publicado` | `{"cuenta","post","enlace"?,"fecha"?}` → la línea nueva |
//!
//! Dos defensas baratas contra una página ajena que le hable a localhost: el
//! `Host` tiene que ser `127.0.0.1:<puerto>` o `localhost:<puerto>` (contra el
//! DNS rebinding), y todo POST viaja como `application/json` (un formulario
//! ajeno no puede mandarlo sin una consulta CORS previa, que aquí no se
//! contesta). [`responder`] es todo el enrutador y no toca sockets, así que se
//! prueba sin red.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

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
}

impl Respuesta {
    fn json(estado: u16, v: &Value) -> Respuesta {
        Respuesta {
            estado,
            tipo: "application/json; charset=utf-8",
            cuerpo: v.to_string().into_bytes(),
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
            }
        }
        ("GET", "/api/estado") => estado(vault, hoy),
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
            Ok(x) => acciones::pedir(vault, &x.cuenta, &x.idea, hoy).map(|(e, repo)| {
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
        (_, "/" | "/api/estado" | "/api/medicion" | "/api/seguidores" | "/api/pedido" | "/api/publicado") => {
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

const MAX_CUERPO: usize = 64 * 1024;

fn leer_peticion(s: &TcpStream) -> Result<Peticion, String> {
    let mut r = BufReader::new(s);
    let mut linea = String::new();
    r.read_line(&mut linea).map_err(|e| e.to_string())?;
    let mut partes = linea.split_whitespace();
    let metodo = partes.next().ok_or("petición vacía")?.to_string();
    let objetivo = partes.next().ok_or("sin ruta")?;
    let ruta = objetivo.split('?').next().unwrap_or("/").to_string();
    let (mut host, mut tipo, mut largo) = (String::new(), String::new(), 0usize);
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
        host,
        tipo,
        cuerpo,
    })
}

fn escribir(mut s: &TcpStream, r: &Respuesta) -> std::io::Result<()> {
    let razon = match r.estado {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        415 => "Unsupported Media Type",
        _ => "Error",
    };
    write!(
        s,
        "HTTP/1.1 {} {razon}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
        r.estado,
        r.tipo,
        r.cuerpo.len()
    )?;
    s.write_all(&r.cuerpo)?;
    s.flush()
}

fn atender(vault: &Vault, puerto: u16, s: &TcpStream) {
    let r = match leer_peticion(s) {
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
    for s in l.incoming().flatten() {
        atender(vault, puerto, &s);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn pet(metodo: &str, ruta: &str, cuerpo: &str) -> Peticion {
        Peticion {
            metodo: metodo.into(),
            ruta: ruta.into(),
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
}
