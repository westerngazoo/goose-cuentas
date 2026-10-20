//! `goose-cuentas` — la administración local de las cuentas de la casa.
//!
//! Tres capas, y esta es la tercera:
//!
//! 1. **El núcleo** es guion-video-creator: hace los videos y lee los posts
//!    (`guion-tablero`). Aquí se consume, no se copia.
//! 2. **Las cuentas son datos:** cada repo declara las suyas en
//!    `guion-cuentas.toml`, y el contenido se queda en su repo.
//! 3. **La administración** es esto: el registro de las cuentas, las
//!    estadísticas que el dueño captura a mano y los pedidos. Todo vive en el
//!    vault personal ([`vault`]), en archivos de texto que se leen sin esta
//!    herramienta.
//!
//! Nada de aquí publica en Instagram ni lee su API: los números son los que el
//! dueño copia de Instagram, con la fecha en que los copió.

pub mod acciones;
pub mod estadisticas;
pub mod publicado;
pub mod registro;
pub mod resumen;
pub mod ui;
pub mod vault;

/// Un identificador corto: minúsculas ASCII, dígitos y guiones, sin guion al
/// principio ni al final (`rotor`, `curl-barra-polea`).
pub fn slug_valido(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('-')
        && !s.ends_with('-')
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

#[cfg(test)]
mod tests {
    use super::slug_valido;

    #[test]
    fn slugs() {
        assert!(slug_valido("rotor"));
        assert!(slug_valido("curl-barra-polea"));
        assert!(slug_valido("04"));
        assert!(!slug_valido(""));
        assert!(!slug_valido("-x"));
        assert!(!slug_valido("Rotor"));
        assert!(!slug_valido("fuerza g"));
        assert!(!slug_valido("física"));
    }
}
