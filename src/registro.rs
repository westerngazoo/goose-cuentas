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
        Registro::parse(&t, base, inicio().as_deref())
            .map_err(|e| format!("{}: {e}", ruta.display()))
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
