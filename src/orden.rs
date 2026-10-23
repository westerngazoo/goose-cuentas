//! `publicar/ORDEN.md`: el orden en que se suben los reels de cada cuenta.
//!
//! El archivo lo escriben los agentes de las cuentas; aquí se lee tal cual y
//! solo se reescribe la línea de estado de una entrada. Su forma:
//!
//! ```text
//! ## @cuenta                      ← una sección por cuenta
//! (texto: notas de la sección)
//! ### 1. Título                   ← o ####; el número es opcional
//! - **Carpeta:** `publicar/…/tema/`
//! **Caption** … ```text … ```
//! <summary>Texto alternativo …</summary> ```text … ```
//! - [ ] Publicado · fecha: ______ · enlace: ______
//! ```
//!
//! Una entrada es un encabezado seguido de su `**Carpeta:**`; los encabezados
//! sin carpeta («Diseño nuevo…», «Después…») solo agrupan.

use serde::Serialize;

/// Una entrada: un reel por subir (o ya subido).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Entrada {
    pub titulo: String,
    /// La carpeta del paquete, relativa a la raíz del repo, sin `/` final.
    pub carpeta: String,
    pub publicado: bool,
    pub fecha: Option<String>,
    pub enlace: Option<String>,
    /// El caption y el texto alternativo tal como vienen en ORDEN.md.
    pub caption: Option<String>,
    pub alt: Option<String>,
    /// El índice de la línea `- [ ] Publicado …` (para reescribirla).
    #[serde(skip)]
    pub linea_estado: Option<usize>,
}

/// Una sección (una cuenta).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Seccion {
    pub titulo: String,
    /// El texto antes de la primera entrada (avisos, «primero el perfil»…).
    pub nota: String,
    pub entradas: Vec<Entrada>,
}

fn quitar_numero(t: &str) -> String {
    let t = t.trim();
    let sin = t.trim_start_matches(|c: char| c.is_ascii_digit());
    match sin.strip_prefix(". ") {
        Some(r) if sin.len() < t.len() => r.trim().to_string(),
        _ => t.to_string(),
    }
}

fn entre_comillas(linea: &str) -> Option<String> {
    let a = linea.find('`')?;
    let b = linea[a + 1..].find('`')?;
    Some(linea[a + 1..a + 1 + b].trim_end_matches('/').to_string())
}

/// `fecha: X · enlace: Y` de la línea de estado; los huecos (`______`, vacío,
/// `(pendiente)`) son `None`.
fn estado(linea: &str) -> (bool, Option<String>, Option<String>) {
    let publicado = linea.contains("[x]") || linea.contains("[X]");
    let campo = |k: &str| {
        let i = linea.find(k)? + k.len();
        let resto = &linea[i..];
        let fin = resto.find('·').unwrap_or(resto.len());
        let v = resto[..fin].trim();
        (!v.is_empty() && !v.chars().all(|c| c == '_') && v != "(pendiente)").then(|| v.to_string())
    };
    (publicado, campo("fecha:"), campo("enlace:"))
}

fn es_estado(linea: &str) -> bool {
    let t = linea.trim_start();
    (t.starts_with("- [ ]") || t.starts_with("- [x]") || t.starts_with("- [X]"))
        && t.contains("Publicado")
}

/// Lee el texto de ORDEN.md.
pub fn parse(texto: &str) -> Vec<Seccion> {
    let lineas: Vec<&str> = texto.lines().collect();
    let mut secciones: Vec<Seccion> = Vec::new();
    let mut encabezado: Option<String> = None;
    let mut i = 0;
    // a qué bloque ```text va lo que sigue: el caption o el alt
    let mut siguiente_bloque: Option<bool> = None; // Some(true) = caption, Some(false) = alt
    while i < lineas.len() {
        let l = lineas[i];
        if let Some(t) = l.strip_prefix("## ") {
            secciones.push(Seccion {
                titulo: t.trim().to_string(),
                nota: String::new(),
                entradas: Vec::new(),
            });
            encabezado = None;
            siguiente_bloque = None;
        } else if let Some(t) = l.strip_prefix("#### ").or_else(|| l.strip_prefix("### ")) {
            encabezado = Some(quitar_numero(t));
            siguiente_bloque = None;
        } else if let Some(s) = secciones.last_mut() {
            if l.contains("**Carpeta:**") {
                if let (Some(t), Some(c)) = (encabezado.take(), entre_comillas(l)) {
                    s.entradas.push(Entrada {
                        titulo: t,
                        carpeta: c,
                        publicado: false,
                        fecha: None,
                        enlace: None,
                        caption: None,
                        alt: None,
                        linea_estado: None,
                    });
                }
            } else if l.starts_with("**Caption**") {
                siguiente_bloque = Some(true);
            } else if l.contains("<summary>") && l.contains("Texto alternativo") {
                siguiente_bloque = Some(false);
            } else if l.trim_start().starts_with("```text") {
                let mut bloque = Vec::new();
                i += 1;
                while i < lineas.len() && !lineas[i].trim_start().starts_with("```") {
                    bloque.push(lineas[i]);
                    i += 1;
                }
                let texto = bloque.join("\n").trim().to_string();
                if let (Some(es_caption), Some(e)) =
                    (siguiente_bloque.take(), s.entradas.last_mut())
                {
                    if e.linea_estado.is_none() {
                        if es_caption {
                            e.caption = Some(texto);
                        } else {
                            e.alt = Some(texto);
                        }
                    }
                }
            } else if es_estado(l) {
                if let Some(e) = s.entradas.last_mut() {
                    if e.linea_estado.is_none() {
                        let (p, f, en) = estado(l);
                        e.publicado = p;
                        e.fecha = f;
                        e.enlace = en;
                        e.linea_estado = Some(i);
                    }
                }
            } else if s.entradas.is_empty()
                && encabezado.is_none()
                && l.trim() != "---"
                && (!s.nota.is_empty() || !l.trim().is_empty())
            {
                s.nota.push_str(l);
                s.nota.push('\n');
            }
        }
        i += 1;
    }
    for s in &mut secciones {
        s.nota = s.nota.trim().to_string();
    }
    secciones
}

/// Si `texto` tiene una entrada con `carpeta` (con o sin `/` al final).
pub fn tiene(texto: &str, carpeta: &str) -> bool {
    let carpeta = carpeta.trim_end_matches('/');
    parse(texto)
        .into_iter()
        .flat_map(|s| s.entradas)
        .any(|e| e.carpeta == carpeta)
}

/// El texto con la entrada de `carpeta` marcada publicada en `fecha`, con su
/// enlace si lo hay. Si ya estaba publicada, solo completa el enlace.
pub fn marcar(
    texto: &str,
    carpeta: &str,
    fecha: &str,
    enlace: Option<&str>,
) -> Result<String, String> {
    let carpeta = carpeta.trim_end_matches('/');
    let e = parse(texto)
        .into_iter()
        .flat_map(|s| s.entradas)
        .find(|e| e.carpeta == carpeta)
        .ok_or_else(|| format!("ORDEN.md no tiene la carpeta «{carpeta}»"))?;
    let n = e
        .linea_estado
        .ok_or_else(|| format!("«{}» no tiene línea «Publicado» en ORDEN.md", e.titulo))?;
    if e.publicado && (enlace.is_none() || e.enlace.is_some()) {
        return Err(format!("«{}» ya está marcado publicado", e.titulo));
    }
    let fecha = if e.publicado {
        e.fecha.clone().unwrap_or_else(|| fecha.to_string())
    } else {
        fecha.to_string()
    };
    let lineas: Vec<&str> = texto.split_inclusive('\n').collect();
    let vieja = lineas[n];
    let sangria = &vieja[..vieja.len() - vieja.trim_start().len()];
    let fin = &vieja[vieja.trim_end_matches(['\n', '\r']).len()..];
    let nueva = format!(
        "{sangria}- [x] Publicado · fecha: {fecha} · enlace: {}{fin}",
        enlace.unwrap_or("(pendiente)")
    );
    let mut out = String::with_capacity(texto.len() + 64);
    for (i, l) in lineas.iter().enumerate() {
        out.push_str(if i == n { &nueva } else { l });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORDEN: &str = "# Orden

Texto general.

---

## @rotorfisicoracing

Ya publicados: varios.

### 1. ¿Por qué frena? (nivel 1)

- **Carpeta:** `publicar/rotor/nivel-1/frenar/`
- **Video:** `frenar.mp4`

**Caption** (pégalo tal cual):

```text
¿Por qué frena?

#f1 #karting
```

<details><summary>Texto alternativo (Configuración avanzada → Accesibilidad)</summary>

```text
Un kart frena.
```

</details>

- [x] Publicado · fecha: 2026-10-19 · enlace: (pendiente)

### 2. La rueda

- **Carpeta:** `publicar/rotor/nivel-1/rueda/`

- [ ] Publicado · fecha: ______ · enlace: ______

### Después (todavía no se sube)

Nada.

## Physicus Mathematicus Magiae Obscurae

> **Alto:** no subas lo viejo.

### Diseño nuevo

#### El intro (primero)

- **Carpeta:** `publicar/pmmo/intro/`

- [ ] Publicado · fecha: · enlace:
";

    #[test]
    fn lee_secciones_entradas_y_estado() {
        let s = parse(ORDEN);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].titulo, "@rotorfisicoracing");
        assert_eq!(s[0].nota, "Ya publicados: varios.");
        assert_eq!(s[0].entradas.len(), 2);
        let e = &s[0].entradas[0];
        assert_eq!(e.titulo, "¿Por qué frena? (nivel 1)");
        assert_eq!(e.carpeta, "publicar/rotor/nivel-1/frenar");
        assert!(e.publicado);
        assert_eq!(e.fecha.as_deref(), Some("2026-10-19"));
        assert_eq!(e.enlace, None);
        assert_eq!(
            e.caption.as_deref(),
            Some("¿Por qué frena?\n\n#f1 #karting")
        );
        assert_eq!(e.alt.as_deref(), Some("Un kart frena."));
        assert!(!s[0].entradas[1].publicado);
        assert_eq!(s[0].entradas[1].fecha, None);
        assert_eq!(s[1].entradas[0].titulo, "El intro (primero)");
        assert!(s[1].nota.contains("Alto"));
    }

    #[test]
    fn marca_y_completa_el_enlace() {
        let t = marcar(ORDEN, "publicar/rotor/nivel-1/rueda/", "2026-10-21", None).unwrap();
        assert!(
            t.contains("- [x] Publicado · fecha: 2026-10-21 · enlace: (pendiente)\n\n### Después")
        );
        let e = &parse(&t)[0].entradas[1];
        assert!(e.publicado);
        let u = marcar(
            &t,
            "publicar/rotor/nivel-1/rueda",
            "2026-10-22",
            Some("https://www.instagram.com/reel/x/"),
        )
        .unwrap();
        let e = &parse(&u)[0].entradas[1];
        assert_eq!(e.fecha.as_deref(), Some("2026-10-21"));
        assert_eq!(
            e.enlace.as_deref(),
            Some("https://www.instagram.com/reel/x/")
        );
        assert!(
            marcar(&u, "publicar/rotor/nivel-1/rueda", "2026-10-22", None)
                .unwrap_err()
                .contains("ya está")
        );
        assert!(marcar(ORDEN, "publicar/x", "2026-10-22", None)
            .unwrap_err()
            .contains("no tiene la carpeta"));
        // el resto del archivo no cambia
        assert_eq!(u.lines().count(), ORDEN.lines().count());
    }
}
