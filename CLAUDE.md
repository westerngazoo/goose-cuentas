# CLAUDE.md — goose-cuentas

La administración local de las cuentas de la casa. El dueño es Gustavo
Delgadillo (westerngazoo) y tiene la última palabra. Antes de cambiar algo,
lee `README.md`, `docs/plan.md` y `docs/decisions.md`.

## Reglas que no se rompen

1. **Nada se publica sin el dueño.** Esta herramienta no sube nada a Instagram
   y no tiene credenciales. `cuentas publicado` solo se corre cuando el dueño
   dice que ya subió el post, nunca por inferencia.
2. **Los números tienen procedencia.** Todo lo de `estadisticas/` es
   *reportado por Instagram y copiado a mano* en su `fecha`. Lo que se calcula
   aquí (la interacción, los cambios) se dice calculado. Una celda vacía nunca
   se muestra como cero.
3. **El vault es del dueño y vive en su máquina.** Este repo no guarda
   estadísticas, ni el registro, ni notas del vault. Las pruebas usan carpetas
   temporales. Nada de credenciales ni de datos de terceros en el vault.
4. **El contenido se queda en el repo de su cuenta.** Aquí se lee con
   `guion-tablero`. Lo genérico que falte va a guion-video-creator, no aquí.
5. **No se implementa la API de Instagram** (ni de lectura) sin que el dueño
   cierre OQ-4.

## Cómo está hecho

- `src/vault.rs`: dónde está el vault (`--vault`, `$GOOSE_VAULT`, el config,
  `~/vaults/personal`) y lo que `iniciar` escribe.
- `src/registro.rs`: `cuentas.toml`, con los repos resueltos (`~/` incluido).
- `src/estadisticas.rs`: los dos CSV. Solo se agregan filas, y lo escrito se
  vuelve a leer antes de guardarlo.
- `src/resumen.rs`: el tablero (texto, JSON y `resumen.md`).
- `src/publicado.rs`: la línea `estado:` del README, con lo anterior detrás.
- `src/orden.rs`: lee `publicar/ORDEN.md` (secciones, entradas, caption, alt, estado) y reescribe solo la línea «Publicado» de una entrada.
- `src/acciones.rs`: lo que hacen la línea de comandos y la interfaz, lo mismo para las dos.
- `src/ui.rs` y `src/ui.html`: `cuentas ui`. Solo std, atado a 127.0.0.1, con el `Host` comprobado y los POST en JSON. Sin CDN. `/archivo` solo sirve MP4, PNG y TXT de dentro de `publicar/`, con `Range` para el video.
- `src/main.rs`: la línea de comandos.

Antes de un commit: `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings` y `cargo test`. Rust estable.
Commits con `git commit -- <rutas>`, sin force-push. Nada que rompa un clon
nuevo.
