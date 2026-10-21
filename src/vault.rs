//! El vault personal: dónde está y qué pone esta herramienta en él.
//!
//! El vault vive en la máquina del dueño (por ejemplo `~/vault/vault personal`),
//! no en un repo. Se busca así: `--vault DIR`, si no `$GOOSE_VAULT`, si no el
//! que guardó `cuentas iniciar --vault DIR` en `~/.config/goose-cuentas/config.toml`,
//! si no el primero que exista de [`CANDIDATOS`]. Esta herramienta solo escribe
//! dentro de su carpeta (`cuentas/`, o la `carpeta` del config):
//!
//! ```text
//! cuentas/
//!   README.md                   qué es cada archivo (lo escribe `iniciar`)
//!   cuentas.toml                el registro de las cuentas
//!   estadisticas/posts.csv      una fila por medición de un post
//!   estadisticas/seguidores.csv una fila por medición de los seguidores
//!   resumen.md                  generado: el tablero para leerlo en el vault
//! ```
//!
//! Todo es Markdown con frontmatter, TOML o CSV: lo lee Obsidian, un plugin de
//! segundo cerebro o cualquier editor, sin esta herramienta.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::estadisticas::{ENCABEZADO_POSTS, ENCABEZADO_SEGUIDORES};

/// La variable que nombra el vault.
pub const ENV: &str = "GOOSE_VAULT";
/// Dónde se busca el vault si nada lo nombra, relativo a la carpeta de inicio.
pub const CANDIDATOS: [&str; 6] = [
    "vaults/personal",
    "vaults/vault personal",
    "Vaults/personal",
    "vault/vault personal",
    "vault/personal",
    "vault personal",
];
/// Las carpetas que suelen juntar varios vaults: de ellas se toma la que
/// tenga «personal» en el nombre.
pub const CONTENEDORES: [&str; 5] = ["vaults", "Vaults", "vault", "Vault", "Documents/vaults"];

/// Dónde guarda Obsidian su lista de vaults (`obsidian.json`), según el
/// sistema: macOS, Linux (normal y Flatpak) y Windows.
pub fn obsidian_json(h: &Path) -> Vec<PathBuf> {
    let mut v = vec![
        h.join("Library/Application Support/obsidian/obsidian.json"),
        h.join(".config/obsidian/obsidian.json"),
        h.join(".var/app/md.obsidian.Obsidian/config/obsidian/obsidian.json"),
    ];
    if let Some(a) = std::env::var_os("APPDATA").filter(|a| !a.is_empty()) {
        v.push(PathBuf::from(a).join("obsidian/obsidian.json"));
    }
    v
}

/// Los vaults que Obsidian conoce: las rutas de `obsidian.json`
/// (`{"vaults": {"<id>": {"path": "…"}}}`), las que existan.
pub fn vaults_de_obsidian(h: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for f in obsidian_json(h) {
        let Ok(t) = std::fs::read_to_string(&f) else {
            continue;
        };
        let Ok(j) = serde_json::from_str::<serde_json::Value>(&t) else {
            continue;
        };
        let Some(vaults) = j.get("vaults").and_then(|v| v.as_object()) else {
            continue;
        };
        for v in vaults.values() {
            if let Some(p) = v.get("path").and_then(|p| p.as_str()) {
                let p = PathBuf::from(p);
                if p.is_dir() && !out.contains(&p) {
                    out.push(p);
                }
            }
        }
    }
    out
}

/// Todos los vaults que se encuentran: los de Obsidian, más cualquier carpeta
/// con `.obsidian/` adentro, directamente bajo `h` o bajo un contenedor.
pub fn descubrir(h: &Path) -> Vec<PathBuf> {
    let mut out = vaults_de_obsidian(h);
    let mut mirar = |d: PathBuf| {
        let Ok(it) = std::fs::read_dir(&d) else {
            return;
        };
        for e in it.flatten() {
            let p = e.path();
            if p.is_dir() && p.join(".obsidian").is_dir() && !out.contains(&p) {
                out.push(p);
            }
        }
    };
    mirar(h.to_path_buf());
    for c in CONTENEDORES {
        mirar(h.join(c));
    }
    // sin ningún vault reconocible, las carpetas de los contenedores tal cual:
    // un vault que no se ha abierto en Obsidian no tiene .obsidian/ todavía
    if out.is_empty() {
        for c in CONTENEDORES {
            let Ok(it) = std::fs::read_dir(h.join(c)) else {
                continue;
            };
            for e in it.flatten() {
                let p = e.path();
                let oculta = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with('.'));
                if p.is_dir() && !oculta && !out.contains(&p) {
                    out.push(p);
                }
            }
        }
    }
    out.sort();
    out
}

fn dice_personal(p: &Path) -> bool {
    p.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.to_lowercase().contains("personal"))
}

/// El vault personal bajo `h`: un candidato que exista; si no, dentro de un
/// contenedor, una carpeta con «personal» en el nombre; si no, entre los
/// vaults descubiertos ([`descubrir`]), el que diga «personal», o el único
/// que haya.
pub fn buscar(h: &Path) -> Option<PathBuf> {
    if let Some(p) = CANDIDATOS.iter().map(|c| h.join(c)).find(|p| p.is_dir()) {
        return Some(p);
    }
    let en_contenedor = CONTENEDORES.iter().find_map(|c| {
        let mut hijos: Vec<PathBuf> = std::fs::read_dir(h.join(c))
            .ok()?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir() && dice_personal(p))
            .collect();
        hijos.sort();
        hijos.into_iter().next()
    });
    if en_contenedor.is_some() {
        return en_contenedor;
    }
    let todos = descubrir(h);
    let personales: Vec<&PathBuf> = todos.iter().filter(|p| dice_personal(p)).collect();
    match (personales.len(), todos.len()) {
        (1, _) => Some(personales[0].clone()),
        (0, 1) => Some(todos[0].clone()),
        _ => None,
    }
}

/// El texto del error cuando no se encuentra el vault: dónde se buscó y, si
/// se descubrió alguno, la lista numerada para elegir.
pub fn explicar_no_encontrado(h: &Path) -> String {
    let mut m = format!(
        "no encuentro el vault personal (busqué ~/{}; dentro de ~/{}, una carpeta con \
         «personal» en el nombre; y la lista de vaults de Obsidian)",
        CANDIDATOS.join(", ~/"),
        CONTENEDORES.join(", ~/")
    );
    let v = descubrir(h);
    if v.is_empty() {
        m.push_str(".\nNómbralo una vez con `cuentas iniciar --vault \"<ruta>\"`.");
    } else {
        m.push_str(".\nEstos vaults sí encontré:\n");
        for (i, p) in v.iter().enumerate() {
            m.push_str(&format!("  {}. {}\n", i + 1, p.display()));
        }
        m.push_str("Corre `cuentas iniciar` y elige uno, o `cuentas iniciar --vault \"<ruta>\"`.");
    }
    m
}

/// La carpeta de esta herramienta dentro del vault, si el config no dice otra.
pub const CARPETA: &str = "cuentas";

/// `~/.config/goose-cuentas/config.toml`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub carpeta: Option<String>,
}

/// La carpeta de inicio: `$HOME`, si no `%USERPROFILE%` (Windows).
pub fn inicio() -> Option<PathBuf> {
    ["HOME", "USERPROFILE"]
        .iter()
        .find_map(|k| std::env::var_os(k).filter(|v| !v.is_empty()))
        .map(PathBuf::from)
}

/// `~/x` es `inicio/x`; lo demás se queda igual.
pub fn expandir(ruta: &str, inicio: Option<&Path>) -> Result<PathBuf, String> {
    match ruta.strip_prefix("~/").or_else(|| ruta.strip_prefix("~\\")) {
        Some(r) => inicio
            .map(|h| h.join(r))
            .ok_or_else(|| format!("«{ruta}»: no sé cuál es la carpeta de inicio")),
        None if ruta == "~" => inicio
            .map(Path::to_path_buf)
            .ok_or_else(|| "«~»: no sé cuál es la carpeta de inicio".to_string()),
        None => Ok(PathBuf::from(ruta)),
    }
}

impl Config {
    pub fn ruta(inicio: &Path) -> PathBuf {
        inicio.join(".config/goose-cuentas/config.toml")
    }

    /// Lee el config; sin archivo, el vacío.
    pub fn leer(ruta: &Path) -> Result<Config, String> {
        match std::fs::read_to_string(ruta) {
            Ok(t) => toml::from_str(&t).map_err(|e| format!("{}: {e}", ruta.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(format!("{}: {e}", ruta.display())),
        }
    }

    pub fn guardar(&self, ruta: &Path) -> Result<(), String> {
        if let Some(d) = ruta.parent() {
            std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        let t = format!(
            "# goose-cuentas: dónde está el vault personal (lo escribe `cuentas iniciar --vault`).\n{}",
            toml::to_string(self).map_err(|e| e.to_string())?
        );
        std::fs::write(ruta, t).map_err(|e| format!("{}: {e}", ruta.display()))
    }
}

/// El vault y sus rutas.
#[derive(Debug, Clone)]
pub struct Vault {
    pub raiz: PathBuf,
    pub carpeta: String,
}

impl Vault {
    /// `flag`, si no `$GOOSE_VAULT`, si no el config, si no el primer
    /// candidato que exista bajo `inicio`.
    pub fn localizar(
        flag: Option<&Path>,
        config: &Config,
        inicio: Option<&Path>,
    ) -> Result<Vault, String> {
        let carpeta = config
            .carpeta
            .clone()
            .unwrap_or_else(|| CARPETA.to_string());
        let raiz = if let Some(f) = flag {
            f.to_path_buf()
        } else if let Some(v) = std::env::var_os(ENV).filter(|v| !v.is_empty()) {
            PathBuf::from(v)
        } else if let Some(v) = &config.vault {
            expandir(v, inicio)?
        } else {
            let h =
                inicio.ok_or("no sé cuál es la carpeta de inicio; nombra el vault con --vault")?;
            buscar(h).ok_or_else(|| explicar_no_encontrado(h))?
        };
        Ok(Vault { raiz, carpeta })
    }

    pub fn cuentas(&self) -> PathBuf {
        self.raiz.join(&self.carpeta)
    }

    pub fn registro(&self) -> PathBuf {
        self.cuentas().join("cuentas.toml")
    }

    pub fn posts_csv(&self) -> PathBuf {
        self.cuentas().join("estadisticas/posts.csv")
    }

    pub fn seguidores_csv(&self) -> PathBuf {
        self.cuentas().join("estadisticas/seguidores.csv")
    }

    pub fn resumen_md(&self) -> PathBuf {
        self.cuentas().join("resumen.md")
    }

    /// Crea lo que falte de la carpeta (nunca pisa un archivo) y devuelve lo
    /// que creó. El vault tiene que existir.
    pub fn iniciar(&self) -> Result<Vec<PathBuf>, String> {
        if !self.raiz.is_dir() {
            return Err(format!(
                "no existe el vault {} (nómbralo con --vault o ${ENV})",
                self.raiz.display()
            ));
        }
        let archivos = [
            (self.cuentas().join("README.md"), README.to_string()),
            (self.registro(), REGISTRO.to_string()),
            (self.posts_csv(), format!("{ENCABEZADO_POSTS}\n")),
            (self.seguidores_csv(), format!("{ENCABEZADO_SEGUIDORES}\n")),
        ];
        let mut creados = Vec::new();
        for (ruta, texto) in archivos {
            if ruta.exists() {
                continue;
            }
            if let Some(dir) = ruta.parent() {
                std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            }
            std::fs::write(&ruta, texto).map_err(|e| format!("{}: {e}", ruta.display()))?;
            creados.push(ruta);
        }
        Ok(creados)
    }
}

/// `cuentas/README.md`.
pub const README: &str = "\
---
tags: [cuentas]
---

# Cuentas

La administración de las cuentas de la casa. Lo lee y lo escribe `cuentas`
(el proyecto goose-cuentas), y todo se puede editar a mano.

| Archivo | Qué es |
|---|---|
| `cuentas.toml` | El registro: cada cuenta, su usuario de Instagram y el repo donde vive su contenido. |
| `estadisticas/posts.csv` | Una fila por medición de un post. |
| `estadisticas/seguidores.csv` | Una fila por medición de los seguidores de una cuenta. |
| `resumen.md` | Generado por `cuentas nota`. No se edita a mano. |

## Procedencia de los números

Todo número de `estadisticas/` es **reportado por Instagram** (la app o
Insights) y **copiado a mano** por el dueño en la `fecha` de su fila. No es un
número medido por nosotros ni modelado. Una medición nueva es una fila nueva:
nunca se corrige una fila vieja para que diga otra fecha, y el resumen usa la
más reciente de cada post.

Columnas de `posts.csv`: `fecha` (AAAA-MM-DD, cuando se copió), `cuenta` (el
`slug` de `cuentas.toml`), `post` (la carpeta del post en su repo, o un slug
para los posts viejos que no tienen carpeta), y luego `vistas`, `alcance`,
`likes`, `comentarios`, `guardados`, `compartidos` y `seguidores_ganados`. Una
celda vacía es «no se copió», no cero. `nota` es texto libre.

## Lo que no va aquí

- Contraseñas, tokens o cualquier credencial de Instagram.
- Datos de terceros: nombres, caras o números de otras personas.
- El contenido de los posts: se queda en el repo de su cuenta.
";

/// `cuentas/cuentas.toml` de la casa.
pub const REGISTRO: &str = r#"# El registro de las cuentas de la casa (goose-cuentas).
#
# slug      — el identificador corto que usan las estadísticas y la línea de comandos.
# nombre    — la cuenta tal como la declara el `guion-cuentas.toml` de su repo.
# instagram — el usuario, sin @ (vacío mientras no esté reservado).
# repo      — el repo donde vive su contenido: `~/` es la carpeta de inicio; una ruta
#             relativa es relativa a esta carpeta.
version = 1

[[cuenta]]
slug = "rotor"
nombre = "Rotor Físico"
instagram = "rotorfisicoracing"
repo = "~/code/rotorf-sico"

[[cuenta]]
slug = "fbf"
nombre = "fisicobuenfisico"
instagram = "fisicobuenfisico"
repo = "~/code/fisicobuenfisico"

[[cuenta]]
slug = "pmmo"
nombre = "PMMO"
instagram = ""
repo = "~/code/rotorf-sico"

[[cuenta]]
slug = "computar"
nombre = "El extraño arte de computar"
instagram = ""
repo = "~/code/rotorf-sico"
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_flag_manda() {
        let v = Vault::localizar(Some(Path::new("/v")), &Config::default(), None).unwrap();
        assert_eq!(v.raiz, PathBuf::from("/v"));
        assert_eq!(
            v.posts_csv(),
            PathBuf::from("/v/cuentas/estadisticas/posts.csv")
        );
    }

    #[test]
    fn el_config_y_los_candidatos() {
        if std::env::var_os(ENV).is_some() {
            return; // el entorno manda sobre el config
        }
        let c = Config {
            vault: Some("~/notas/vault personal".into()),
            carpeta: Some("Areas/Cuentas".into()),
        };
        let v = Vault::localizar(None, &c, Some(Path::new("/home/g"))).unwrap();
        assert_eq!(v.raiz, PathBuf::from("/home/g/notas/vault personal"));
        assert_eq!(
            v.registro(),
            PathBuf::from("/home/g/notas/vault personal/Areas/Cuentas/cuentas.toml")
        );

        let h = std::env::temp_dir().join(format!("goose-cuentas-inicio-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&h);
        assert!(Vault::localizar(None, &Config::default(), Some(&h))
            .unwrap_err()
            .contains("--vault"));
        std::fs::create_dir_all(h.join("vault/vault personal")).unwrap();
        let v = Vault::localizar(None, &Config::default(), Some(&h)).unwrap();
        assert_eq!(v.raiz, h.join("vault/vault personal"));
        let _ = std::fs::remove_dir_all(&h);
    }

    #[test]
    fn el_vault_personal_dentro_de_vaults() {
        let h = std::env::temp_dir().join(format!("goose-cuentas-vaults-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&h);
        std::fs::create_dir_all(h.join("vaults/trabajo")).unwrap();
        std::fs::create_dir_all(h.join("vaults/Mi Personal")).unwrap();
        assert_eq!(buscar(&h), Some(h.join("vaults/Mi Personal")));
        std::fs::create_dir_all(h.join("vaults/personal")).unwrap();
        assert_eq!(buscar(&h), Some(h.join("vaults/personal")));
        let _ = std::fs::remove_dir_all(&h);
    }

    #[test]
    fn descubre_los_vaults_de_obsidian_y_los_de_punto_obsidian() {
        let h = std::env::temp_dir().join(format!("goose-cuentas-obs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&h);
        let a = h.join("Documentos/Goose Notes");
        let b = h.join("otros/trabajo");
        std::fs::create_dir_all(a.join(".obsidian")).unwrap();
        std::fs::create_dir_all(b.join(".obsidian")).unwrap();
        std::fs::create_dir_all(h.join("Library/Application Support/obsidian")).unwrap();
        std::fs::write(
            h.join("Library/Application Support/obsidian/obsidian.json"),
            format!(
                r#"{{"vaults":{{"abc":{{"path":{:?},"ts":1,"open":true}},"def":{{"path":{:?}}},"ghi":{{"path":"/no/existe"}}}}}}"#,
                a.display().to_string(),
                b.display().to_string()
            ),
        )
        .unwrap();
        let v = descubrir(&h);
        assert_eq!(v, vec![a.clone(), b.clone()]);
        // sin «personal» y con dos, no adivina; con uno solo, lo toma
        assert_eq!(buscar(&h), None);
        assert!(explicar_no_encontrado(&h).contains("1. "));
        std::fs::remove_dir_all(&b).unwrap();
        assert_eq!(buscar(&h), Some(a));
        let _ = std::fs::remove_dir_all(&h);
    }

    #[test]
    fn sin_punto_obsidian_lista_las_carpetas_del_contenedor() {
        let h = std::env::temp_dir().join(format!("goose-cuentas-cont-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&h);
        std::fs::create_dir_all(h.join("vaults/goosecode")).unwrap();
        std::fs::create_dir_all(h.join("vaults/happycode")).unwrap();
        std::fs::create_dir_all(h.join("vaults/.trash")).unwrap();
        assert_eq!(
            descubrir(&h),
            vec![h.join("vaults/goosecode"), h.join("vaults/happycode")]
        );
        assert_eq!(buscar(&h), None); // dos, ninguna dice «personal»: se pregunta
        let _ = std::fs::remove_dir_all(&h);
    }

    #[test]
    fn el_config_ida_y_vuelta() {
        let f = std::env::temp_dir().join(format!(
            "goose-cuentas-config-{}/config.toml",
            std::process::id()
        ));
        let c = Config {
            vault: Some("/Users/g/vault/vault personal".into()),
            carpeta: None,
        };
        c.guardar(&f).unwrap();
        assert_eq!(Config::leer(&f).unwrap(), c);
        let _ = std::fs::remove_dir_all(f.parent().unwrap());
    }

    #[test]
    fn el_registro_de_la_casa_se_lee() {
        let r = crate::registro::Registro::parse(
            REGISTRO,
            Path::new("/v/cuentas"),
            Some(Path::new("/home/g")),
        )
        .unwrap();
        assert_eq!(r.cuentas.len(), 4);
        assert_eq!(
            r.repos(),
            [
                PathBuf::from("/home/g/code/rotorf-sico"),
                PathBuf::from("/home/g/code/fisicobuenfisico")
            ]
        );
    }
}
