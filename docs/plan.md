# Plan

Qué se abstrae, en qué orden y qué queda en cada capa.

## Lo que se abstrae

| Pieza | Hoy | Hacia |
|---|---|---|
| Las cuentas | Cuatro carpetas con su manual y su `guion-cuentas.toml` | Un registro (`cuentas.toml` en el vault) que apunta a ellas. Una cuenta nueva es `guion cuenta nueva` más una entrada aquí. |
| El ciclo de un post | La línea `estado:` de cada README, `ORDEN.md` y `PENDIENTES.md` escritos a mano | Los estados se leen de los READMEs (`guion-tablero`). `ORDEN.md` se genera de ahí (fase 4). |
| Pedir un reel | Un mensaje en el chat | `cuentas pedir` → `encargos.toml` del repo de la cuenta → el agente lo toma con la skill `hacer-video` (fase 2: una cola local). |
| La entrega | `tools/publicar.sh` con una función por cuenta | Un empaquetador genérico en guion-video-creator, alimentado por `cuenta.toml` (fase 4). |
| Las estadísticas | Ninguna | Mediciones con fecha en el vault, a mano (fase 0). La API solo si se cierra OQ-4 (fase 3). |

## Fases

0. **Hecho: el núcleo.** El registro, los CSV de estadísticas, el tablero
   (texto, JSON y `resumen.md`), los pedidos y `publicado`, con pruebas.
1. **Hecho: la interfaz local (`cuentas ui`).** Un servidor solo en `127.0.0.1`,
   en Rust, sobre este mismo núcleo, que escribe los mismos archivos. Páginas:
   - **Tablero:** las cuatro cuentas y sus seguidores en el tiempo (una
     gráfica por cuenta), las vistas y la interacción, los posts con más vistas
     y los publicados que todavía no tienen números.
   - **Capturar:** un formulario por post publicado (vistas, alcance, likes,
     comentarios, guardados, compartidos, seguidores ganados), con la fecha de
     hoy, y los seguidores de cada cuenta. Escribe filas nuevas.
   - **Post:** sus mediciones en el tiempo, su estado, su paquete en
     `publicar/` y el botón «ya lo subí» (= `cuentas publicado`, con el enlace).
   - **Pedir un reel:** la cuenta, la idea (la pregunta) y el formato.
1b. **Hecho: la pestaña Publicar.** `ORDEN.md` en la interfaz: el video, copiar el caption y el alt text, bajar los archivos y «Ya lo subí».
2. **Correr los pedidos.** Una cola local que toma un encargo `pendiente` y
   abre una sesión de Claude Code en el repo de la cuenta con la skill
   `hacer-video`. El resultado llega a `listo`, nunca a `publicado`.
3. **Estadísticas automáticas (solo con OQ-4 cerrada).** La lectura de
   Insights con la Graph API, solo de lectura, escribiendo las mismas filas con
   otra `nota` de procedencia.
4. **Más abstracción.** `ORDEN.md` generado a partir de los estados, y el
   empaquetado de `publicar/` genérico en guion-video-creator.
