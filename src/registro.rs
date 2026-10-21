//! `cuentas/cuentas.toml`: qué cuentas hay y dónde vive su contenido.
//!
//! ```toml
//! version = 1
//! [[cuenta]]
//! slug = "rotor"
//! nombre = "Rotor Físico"          # como en el guion-cuentas.toml de su repo
//! instagram = "rotorfisicoracing"  # sin @; vacío si no está reservado
//! repo = "~/code/rotorf-sico"      # `~/`: la carpeta de inicio; relativo: a este archivo
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::slug_valido;
use crate::vault::{expandir, inicio};

/// Una cuenta del registro.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cuenta {
    pub slug: String,
    pub nombre: String,
    #[serde(default)]
    pub instagram: String,
    pub repo: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Archivo {
    version: u32,
    #[serde(default)]
    cuenta: Vec<Cuenta>,
}

/// El registro leído, con sus repos ya resueltos.
#[derive(Debug, Clone)]
pub struct Registro {
    pub cuentas: Vec<Cuenta>,
    repos: Vec<PathBuf>,
}

/// El texto del registro con la línea `repo = "<viejo>"` del bloque de `slug`
/// cambiada a `nuevo`; lo demás queda igual, comentarios incluidos.
pub fn reescribir_repo(texto: &str, slug: &str, viejo: &str, nuevo: &str) -> String {
    let mut en_bloque = false;
    let mut out = String::with_capacity(texto.len());
    for l in texto.split_inclusive('\n') {
        let t = l.trim();
        if t.starts_with("[[cuenta]]") {
            en_bloque = false;
        } else if t.starts_with("slug") && t.contains(&format!("\"{slug}\"")) {
            en_bloque = true;
        }
        if en_bloque && t.starts_with("repo") && t.contains(&format!("\"{viejo}\"")) {
            // solo el valor entre comillas: la sangría y el comentario se quedan
            out.push_str(&l.replacen(
                &format!("\"{viejo}\""),
                &toml::Value::String(nuevo.to_string()).to_string(),
                1,
            ));
            en_bloque = false;
        } else {
            out.push_str(l);
        }
    }
    out
}

/// Las carpetas de proyectos donde se busca un repo que no está donde dice el
/// registro (bajo la carpeta de inicio).
pub const CARPETAS: [&str; 8] = [
    "code",
    "projects",
    "Projects",
    "Code",
    "src",
    "dev",
    "repos",
    "proyectos",
];

/// `r` si existe; si no, el primer `~/<carpeta>/<nombre de r>` que exista; si
/// ninguno, `r` tal cual (y el error dirá esa ruta).
pub fn buscar_repo(r: PathBuf, inicio: Option<&Path>) -> PathBuf {
    if r.is_dir() {
        return r;
    }
    let (Some(h), Some(nombre)) = (inicio, r.file_name()) else {
        return r;
    };
    CARPETAS
        .iter()
        .map(|c| h.join(c).join(nombre))
        .find(|p| p.is_dir())
        .unwrap_or(r)
}

impl Registro {
    /// El texto del registro; `base` es la carpeta del archivo e `inicio`, la
    /// de `~/`.
    pub fn parse(texto: &str, base: &Path, inicio: Option<&Path>) -> Result<Registro, String> {
        let a: Archivo = toml::from_str(texto).map_err(|e| e.to_string())?;
        if a.version != 1 {
            return Err(format!(
                "version = {}: esta herramienta lee la 1",
                a.version
            ));
        }
        for (i, c) in a.cuenta.iter().enumerate() {
            if !slug_valido(&c.slug) {
                return Err(format!("slug «{}»: minúsculas, dígitos y guiones", c.slug));
            }
            if a.cuenta[..i].iter().any(|d| d.slug == c.slug) {
                return Err(format!("slug «{}» repetido", c.slug));
            }
            if c.nombre.trim().is_empty() {
                return Err(format!("«{}»: falta el nombre", c.slug));
            }
            if c.instagram.starts_with('@') {
                return Err(format!("«{}»: el usuario de Instagram va sin @", c.slug));
            }
        }
        let repos = a
            .cuenta
            .iter()
            .map(|c| {
                expandir(&c.repo, inicio)
                    .map(|r| if r.is_absolute() { r } else { base.join(r) })
                    .map(|r| buscar_repo(r, inicio))
                    .map_err(|e| format!("«{}»: {e}", c.slug))
            })
            .collect::<Result<_, _>>()?;
        Ok(Registro {
            cuentas: a.cuenta,
            repos,
        })
    }

    /// Lee el archivo.
    pub fn leer(ruta: &Path) -> Result<Registro, String> {
        let t = std::fs::read_to_string(ruta)
            .map_err(|e| format!("{}: {e} (¿falta `cuentas iniciar`?)", ruta.display()))?;
        let base = ruta.parent().unwrap_or(Path::new("."));
        let inicio = inicio();
        let mut r = Registro::parse(&t, base, inicio.as_deref())
            .map_err(|e| format!("{}: {e}", ruta.display()))?;
        // una cuenta que se mudó de repo (fbf a rotorf-sico/fbf, 2026-10-21):
        // si su repo ya no existe pero otro del registro la declara, el
        // registro se corrige solo y se reescribe esa línea
        let cambios = r.sanar();
        if !cambios.is_empty() {
            let mut nuevo = t.clone();
            for (slug, viejo, nuevo_repo) in &cambios {
                nuevo = reescribir_repo(&nuevo, slug, viejo, nuevo_repo);
                eprintln!(
                    "registro: {slug} ahora vive en {nuevo_repo} (antes {viejo}); corregido en {}",
                    ruta.display()
                );
            }
            if nuevo != t {
                Registro::parse(&nuevo, base, inicio.as_deref())
                    .map_err(|e| format!("{}: al corregir: {e}", ruta.display()))?;
                std::fs::write(ruta, nuevo).map_err(|e| format!("{}: {e}", ruta.display()))?;
            }
        }
        Ok(r)
    }

    /// Las cuentas cuyo repo no existe: si otro repo del registro declara una
    /// cuenta con el mismo nombre en su `guion-cuentas.toml`, se apuntan a
    /// él. Devuelve (slug, repo viejo tal como estaba, repo nuevo tal como
    /// está en el registro).
    pub fn sanar(&mut self) -> Vec<(String, String, String)> {
        let mut cambios = Vec::new();
        let declara = |repo: &Path, nombre: &str| -> bool {
            guion_tablero::cuentas::load(repo)
                .map(|d| {
                    d.cuentas
                        .iter()
                        .any(|c| guion_tablero::tablero::mismo_nombre(&c.nombre, nombre))
                })
                .unwrap_or(false)
        };
        for i in 0..self.cuentas.len() {
            if self.repos[i].join("guion-cuentas.toml").is_file() {
                continue;
            }
            let nombre = self.cuentas[i].nombre.clone();
            let destino = (0..self.cuentas.len())
                .filter(|&j| j != i && self.repos[j] != self.repos[i])
                .find(|&j| declara(&self.repos[j], &nombre));
            if let Some(j) = destino {
                let repo_nuevo = self.cuentas[j].repo.clone();
                let viejo = std::mem::replace(&mut self.cuentas[i].repo, repo_nuevo.clone());
                self.repos[i] = self.repos[j].clone();
                cambios.push((self.cuentas[i].slug.clone(), viejo, repo_nuevo));
            }
        }
        cambios
    }

    /// La cuenta de `slug`; si no hay, el error dice cuáles hay.
    pub fn cuenta(&self, slug: &str) -> Result<&Cuenta, String> {
        self.cuentas.iter().find(|c| c.slug == slug).ok_or_else(|| {
            let hay: Vec<&str> = self.cuentas.iter().map(|c| c.slug.as_str()).collect();
            format!("no hay cuenta «{slug}» (hay: {})", hay.join(", "))
        })
    }

    /// El repo de una cuenta del registro.
    pub fn repo(&self, c: &Cuenta) -> PathBuf {
        let i = self
            .cuentas
            .iter()
            .position(|d| d.slug == c.slug)
            .expect("la cuenta es de este registro");
        self.repos[i].clone()
    }

    /// Los repos, cada uno una vez, en el orden del registro.
    pub fn repos(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = Vec::new();
        for r in &self.repos {
            if !out.contains(r) {
                out.push(r.clone());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errores_con_nombre() {
        let base = Path::new("/v");
        assert!(Registro::parse("version = 2\n", base, None)
            .unwrap_err()
            .contains("version"));
        let dos = "version = 1\n[[cuenta]]\nslug = \"a\"\nnombre = \"A\"\nrepo = \"r\"\n\
                   [[cuenta]]\nslug = \"a\"\nnombre = \"B\"\nrepo = \"r\"\n";
        assert!(Registro::parse(dos, base, None)
            .unwrap_err()
            .contains("repetido"));
        let arroba = "version = 1\n[[cuenta]]\nslug = \"a\"\nnombre = \"A\"\ninstagram = \"@a\"\nrepo = \"r\"\n";
        assert!(Registro::parse(arroba, base, None)
            .unwrap_err()
            .contains("sin @"));
        let extra =
            "version = 1\n[[cuenta]]\nslug = \"a\"\nnombre = \"A\"\nrepo = \"r\"\nclave = 1\n";
        assert!(Registro::parse(extra, base, None)
            .unwrap_err()
            .contains("clave"));
    }

    #[test]
    fn el_repo_se_busca_en_las_carpetas_de_proyectos() {
        let h = std::env::temp_dir().join(format!("goose-cuentas-repos-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&h);
        std::fs::create_dir_all(h.join("projects/rotorf-sico")).unwrap();
        let r = Registro::parse(
            "version = 1\n[[cuenta]]\nslug = \"rotor\"\nnombre = \"R\"\nrepo = \"~/code/rotorf-sico\"\n\
             [[cuenta]]\nslug = \"x\"\nnombre = \"X\"\nrepo = \"~/code/no-existe\"\n",
            Path::new("/v"),
            Some(&h),
        )
        .unwrap();
        assert_eq!(
            r.repo(r.cuenta("rotor").unwrap()),
            h.join("projects/rotorf-sico")
        );
        assert_eq!(r.repo(r.cuenta("x").unwrap()), h.join("code/no-existe"));
        let _ = std::fs::remove_dir_all(&h);
    }

    #[test]
    fn un_repo_mudado_se_corrige_solo() {
        let h = std::env::temp_dir().join(format!("goose-cuentas-sanar-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&h);
        let casa = h.join("code/rotorf-sico");
        std::fs::create_dir_all(&casa).unwrap();
        std::fs::write(
            casa.join("guion-cuentas.toml"),
            "[[cuenta]]\nnombre = \"Rotor Físico\"\nposts = [\"content/posts/*\"]\n\
             [[cuenta]]\nnombre = \"fisicobuenfisico\"\nposts = [\"fbf/reels/*\"]\n",
        )
        .unwrap();
        let v = h.join("vault/cuentas");
        std::fs::create_dir_all(&v).unwrap();
        let texto = "version = 1\n\n[[cuenta]]\nslug = \"rotor\"\nnombre = \"Rotor Físico\"\nrepo = \"~/code/rotorf-sico\"\n\n\
                     [[cuenta]]\nslug = \"fbf\"\nnombre = \"fisicobuenfisico\"\n  repo = \"~/code/fisicobuenfisico\"   # viejo\n";
        std::fs::write(v.join("cuentas.toml"), texto).unwrap();
        let mut r = Registro::parse(texto, &v, Some(&h)).unwrap();
        let cambios = r.sanar();
        assert_eq!(
            cambios,
            vec![(
                "fbf".to_string(),
                "~/code/fisicobuenfisico".to_string(),
                "~/code/rotorf-sico".to_string()
            )]
        );
        assert_eq!(r.repo(r.cuenta("fbf").unwrap()), casa);
        let nuevo = reescribir_repo(
            texto,
            "fbf",
            "~/code/fisicobuenfisico",
            "~/code/rotorf-sico",
        );
        assert!(
            nuevo.contains("  repo = \"~/code/rotorf-sico\"   # viejo\n"),
            "{nuevo}"
        );
        assert!(nuevo.contains(
            "slug = \"rotor\"\nnombre = \"Rotor Físico\"\nrepo = \"~/code/rotorf-sico\""
        ));
        // ya corregido, no cambia nada
        let mut r2 = Registro::parse(&nuevo, &v, Some(&h)).unwrap();
        assert!(r2.sanar().is_empty());
        let _ = std::fs::remove_dir_all(&h);
    }

    #[test]
    fn la_cuenta_que_falta_dice_cuales_hay() {
        let r = Registro::parse(
            "version = 1\n[[cuenta]]\nslug = \"a\"\nnombre = \"A\"\nrepo = \"../r\"\n",
            Path::new("/v/cuentas"),
            None,
        )
        .unwrap();
        assert!(r.cuenta("b").unwrap_err().contains("hay: a"));
        assert_eq!(
            r.repo(r.cuenta("a").unwrap()),
            PathBuf::from("/v/cuentas/../r")
        );
    }
}
