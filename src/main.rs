//! `cuentas` — la línea de comandos de goose-cuentas.

use std::path::{Path, PathBuf};

use std::process::ExitCode;

use goose_cuentas::estadisticas::{self, Medicion, Seguidores};
use goose_cuentas::registro::Registro;
use goose_cuentas::resumen;
use goose_cuentas::vault::{inicio, Config, Vault};
use goose_cuentas::{publicado, slug_valido};
use guion_tablero::tablero::Tablero;

const USO: &str = "\
cuentas — la administración local de las cuentas de la casa

uso: cuentas [--vault DIR] <comando>

  iniciar [--vault DIR]           crea lo que falte de cuentas/ en el vault (no pisa nada);
                                  con --vault, lo recuerda para la próxima
  tablero [--json]                cada cuenta: estados, seguidores y sus posts con más vistas
  nota                            escribe cuentas/resumen.md en el vault

  stats post <cuenta> <post> [--fecha F] [--vistas N] [--alcance N] [--likes N]
             [--comentarios N] [--guardados N] [--compartidos N] [--seguidores N]
             [--nota TEXTO]       una medición de un post, copiada de Instagram
  stats seguidores <cuenta> <N> [--fecha F] [--nota TEXTO]
  stats ver [<cuenta>] [--json]   las mediciones tal como están en el vault

  pedir <cuenta> <idea…>          pide un reel: un encargo en el repo de la cuenta
  pedidos [<cuenta>]              los encargos y su estado
  publicado <cuenta> <post> [--enlace URL] [--fecha F]
                                  el dueño lo subió: marca su README

El vault es --vault, si no $GOOSE_VAULT, si no el que recordó `iniciar`
(~/.config/goose-cuentas/config.toml), si no ~/vault/vault personal.
La fecha por omisión es hoy. <cuenta> es el slug de cuentas/cuentas.toml.
Después de cada medición se reescribe cuentas/resumen.md.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match correr(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("cuentas: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Los argumentos, separados en posicionales y `--clave valor` / `--bandera`.
struct Args {
    pos: Vec<String>,
    opciones: Vec<(String, Option<String>)>,
}

const BANDERAS: [&str; 2] = ["--json", "--help"];

impl Args {
    fn de(v: Vec<String>) -> Result<Args, String> {
        let mut pos = Vec::new();
        let mut opciones = Vec::new();
        let mut it = v.into_iter();
        while let Some(a) = it.next() {
            if a == "-h" || BANDERAS.contains(&a.as_str()) {
                opciones.push((a, None));
            } else if a.starts_with("--") {
                let v = it.next().ok_or_else(|| format!("{a} necesita un valor"))?;
                opciones.push((a, Some(v)));
            } else {
                pos.push(a);
            }
        }
        Ok(Args { pos, opciones })
    }

    fn tiene(&self, k: &str) -> bool {
        self.opciones.iter().any(|(o, _)| o == k)
    }

    fn valor(&self, k: &str) -> Option<&str> {
        self.opciones
            .iter()
            .rev()
            .find(|(o, _)| o == k)
            .and_then(|(_, v)| v.as_deref())
    }

    fn numero(&self, k: &str) -> Result<Option<u64>, String> {
        self.valor(k)
            .map(|v| {
                v.replace(['_', ',', '\u{202f}', ' '], "")
                    .parse::<u64>()
                    .map_err(|_| format!("{k} «{v}»: un entero ≥ 0"))
            })
            .transpose()
    }

    /// Falla si hay una opción que el comando no conoce.
    fn solo(&self, permitidas: &[&str]) -> Result<(), String> {
        match self
            .opciones
            .iter()
            .find(|(o, _)| !permitidas.contains(&o.as_str()))
        {
            Some((o, _)) => Err(format!("opción desconocida {o}")),
            None => Ok(()),
        }
    }
}

fn correr(v: Vec<String>) -> Result<(), String> {
    let a = Args::de(v)?;
    if a.pos.is_empty() || a.tiene("--help") || a.tiene("-h") {
        println!("{USO}");
        return Ok(());
    }
    let casa = inicio();
    let config_ruta = casa.as_deref().map(Config::ruta);
    let config = match &config_ruta {
        Some(r) => Config::leer(r)?,
        None => Config::default(),
    };
    let vault = Vault::localizar(a.valor("--vault").map(Path::new), &config, casa.as_deref())?;
    let hoy = guion_tablero::fecha::hoy();
    let fecha = a.valor("--fecha").unwrap_or(&hoy).to_string();
    let pos: Vec<&str> = a.pos.iter().map(String::as_str).collect();

    match pos.as_slice() {
        ["iniciar"] => {
            a.solo(&["--vault"])?;
            let creados = vault.iniciar()?;
            if let (Some(v), Some(r)) = (a.valor("--vault"), &config_ruta) {
                let raiz = std::fs::canonicalize(v).map_err(|e| format!("{v}: {e}"))?;
                let nuevo = Config {
                    vault: Some(raiz.display().to_string()),
                    ..config.clone()
                };
                if nuevo != config {
                    nuevo.guardar(r)?;
                    println!("guardado en {}: vault = {}", r.display(), raiz.display());
                }
            }
            if creados.is_empty() {
                println!("{} ya tenía todo", vault.cuentas().display());
            }
            for c in creados {
                println!("creado {}", c.display());
            }
            Ok(())
        }
        ["tablero"] => {
            a.solo(&["--vault", "--json"])?;
            let r = armar(&vault, &hoy)?;
            print!(
                "{}",
                if a.tiene("--json") {
                    r.json()
                } else {
                    r.texto()
                }
            );
            Ok(())
        }
        ["nota"] => {
            a.solo(&["--vault"])?;
            escribir_nota(&vault, &hoy)
        }
        ["stats", "post", cuenta, post] => {
            a.solo(&[
                "--vault",
                "--fecha",
                "--vistas",
                "--alcance",
                "--likes",
                "--comentarios",
                "--guardados",
                "--compartidos",
                "--seguidores",
                "--nota",
            ])?;
            let reg = Registro::leer(&vault.registro())?;
            let m = Medicion {
                fecha,
                cuenta: cuenta.to_string(),
                post: post.to_string(),
                vistas: a.numero("--vistas")?,
                alcance: a.numero("--alcance")?,
                likes: a.numero("--likes")?,
                comentarios: a.numero("--comentarios")?,
                guardados: a.numero("--guardados")?,
                compartidos: a.numero("--compartidos")?,
                seguidores_ganados: a.numero("--seguidores")?,
                nota: a.valor("--nota").unwrap_or("").to_string(),
            };
            estadisticas::agregar_medicion(&vault.posts_csv(), &m, &reg)?;
            println!("agregada: {} {} del {}", m.cuenta, m.post, m.fecha);
            escribir_nota(&vault, &hoy)
        }
        ["stats", "seguidores", cuenta, n] => {
            a.solo(&["--vault", "--fecha", "--nota"])?;
            let reg = Registro::leer(&vault.registro())?;
            let seguidores = n
                .replace(['_', ',', '\u{202f}', ' '], "")
                .parse::<u64>()
                .map_err(|_| format!("seguidores «{n}»: un entero ≥ 0"))?;
            let s = Seguidores {
                fecha,
                cuenta: cuenta.to_string(),
                seguidores,
                nota: a.valor("--nota").unwrap_or("").to_string(),
            };
            estadisticas::agregar_seguidores(&vault.seguidores_csv(), &s, &reg)?;
            println!(
                "agregada: {} con {} seguidores el {}",
                s.cuenta, s.seguidores, s.fecha
            );
            escribir_nota(&vault, &hoy)
        }
        ["stats", "ver", resto @ ..] if resto.len() <= 1 => {
            a.solo(&["--vault", "--json"])?;
            let cuenta = resto.first().copied();
            if let Some(c) = cuenta {
                Registro::leer(&vault.registro())?.cuenta(c)?;
            }
            let es = |c: &str| cuenta.is_none_or(|x| x == c);
            let ms: Vec<Medicion> = estadisticas::leer_posts(&vault.posts_csv())?
                .into_iter()
                .filter(|m| es(&m.cuenta))
                .collect();
            let ss: Vec<Seguidores> = estadisticas::leer_seguidores(&vault.seguidores_csv())?
                .into_iter()
                .filter(|s| es(&s.cuenta))
                .collect();
            if a.tiene("--json") {
                let j = serde_json::json!({ "posts": ms, "seguidores": ss });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&j).map_err(|e| e.to_string())?
                );
            } else {
                for s in &ss {
                    println!("{}  {:<10} seguidores {}", s.fecha, s.cuenta, s.seguidores);
                }
                for m in &ms {
                    let f = |v: Option<u64>| v.map(|x| x.to_string()).unwrap_or_else(|| "—".into());
                    println!(
                        "{}  {:<10} {:<24} vistas {} · likes {} · coment. {} · guard. {} · compart. {}",
                        m.fecha, m.cuenta, m.post, f(m.vistas), f(m.likes), f(m.comentarios),
                        f(m.guardados), f(m.compartidos)
                    );
                }
            }
            Ok(())
        }
        ["pedir", cuenta, idea @ ..] if !idea.is_empty() => {
            a.solo(&["--vault"])?;
            let reg = Registro::leer(&vault.registro())?;
            let c = reg.cuenta(cuenta)?;
            let t = tablero_de(&reg.repo(c))?;
            let e = t.encargar(&c.nombre, &idea.join(" "), &hoy, None)?;
            println!(
                "{} para {}: «{}» (en {}/encargos.toml)",
                e.id,
                c.nombre,
                e.idea,
                reg.repo(c).display()
            );
            Ok(())
        }
        ["pedidos", resto @ ..] if resto.len() <= 1 => {
            a.solo(&["--vault"])?;
            let reg = Registro::leer(&vault.registro())?;
            let cuentas = match resto.first() {
                Some(s) => vec![reg.cuenta(s)?.clone()],
                None => reg.cuentas.clone(),
            };
            for c in &cuentas {
                let t = tablero_de(&reg.repo(c))?;
                let Some(tc) = t.cuenta(&c.nombre) else {
                    continue;
                };
                for e in t.encargos_de(tc) {
                    let post = e
                        .post
                        .as_deref()
                        .map(|p| format!(" → {p}"))
                        .unwrap_or_default();
                    println!(
                        "{} {}  {:<10} {:<10} {}{post}",
                        e.id, e.fecha, c.slug, e.estado, e.idea
                    );
                }
            }
            Ok(())
        }
        ["publicado", cuenta, post] => {
            a.solo(&["--vault", "--fecha", "--enlace"])?;
            if !slug_valido(post) {
                return Err(format!("post «{post}»: el nombre de su carpeta"));
            }
            let reg = Registro::leer(&vault.registro())?;
            let c = reg.cuenta(cuenta)?;
            let t = tablero_de(&reg.repo(c))?;
            let tc = t
                .cuenta(&c.nombre)
                .ok_or_else(|| format!("«{}» no está en su guion-cuentas.toml", c.nombre))?;
            let p = tc
                .posts
                .iter()
                .find(|p| p.nombre == *post)
                .ok_or_else(|| format!("{} no tiene el post «{post}»", c.nombre))?;
            let linea = publicado::marcar(&p.dir.join("README.md"), &fecha, a.valor("--enlace"))?;
            println!("{}/README.md\n{linea}", p.ruta);
            println!("(falta el commit en {})", reg.repo(c).display());
            Ok(())
        }
        _ => Err(format!("no entiendo «{}»; cuentas --help", a.pos.join(" "))),
    }
}

fn tablero_de(repo: &Path) -> Result<Tablero, String> {
    Tablero::leer(&[repo.to_path_buf()], &|_: &Path| None)
}

fn armar(vault: &Vault, hoy: &str) -> Result<resumen::Resumen, String> {
    let reg = Registro::leer(&vault.registro())?;
    let ms = estadisticas::leer_posts(&vault.posts_csv())?;
    let ss = estadisticas::leer_seguidores(&vault.seguidores_csv())?;
    let tableros = resumen::leer_tableros(&reg);
    Ok(resumen::armar(&reg, &ms, &ss, &tableros, hoy))
}

fn escribir_nota(vault: &Vault, hoy: &str) -> Result<(), String> {
    let r = armar(vault, hoy)?;
    let ruta: PathBuf = vault.resumen_md();
    std::fs::write(&ruta, r.markdown()).map_err(|e| format!("{}: {e}", ruta.display()))?;
    println!("escrita {}", ruta.display());
    Ok(())
}
