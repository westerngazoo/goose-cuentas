# Decisiones

| Fecha | Decisión | Quién |
|---|---|---|
| 2026-10-20 | La administración de las cuentas sale a un proyecto propio en `~/code`, más abstracto que los repos de cada cuenta, con una parte de interfaz para administrar las cuentas, pedir un reel para una cuenta y llevar las estadísticas de las cuatro. | El dueño |
| 2026-10-20 | **El contenido se queda** en el repo de su cuenta. Este proyecto lo lee y no lo mueve. | El dueño |
| 2026-10-20 | **Estadísticas a mano primero.** La API de Instagram (OQ-4) sigue abierta. | El dueño |
| 2026-10-20 | El proyecto lo crea el agente. Después viene una interfaz para que el dueño vea las estadísticas y capture a mano likes y vistas. | El dueño |
| 2026-10-20 | Se trabaja con el **vault personal** del dueño, que está en su máquina, dentro de su carpeta de usuario (`vault personal`), y que también usa con su plugin de segundo cerebro. GooseBaremetal es trabajo (otro git) y no se referencia aquí. | El dueño |
| 2026-10-20 | El registro, las estadísticas y los pedidos se guardan como texto (TOML, CSV y Markdown con frontmatter) en `cuentas/` dentro del vault: se leen sin la herramienta y se corrigen a mano. Los pedidos van al `encargos.toml` del repo de la cuenta, el mismo que lee `guion tablero`. | El agente, sobre lo anterior |
| 2026-10-20 | **GitHub es para lo personal:** este repo y los de las cuentas viven en GitHub (westerngazoo). Lo del trabajo (GooseBaremetal, happydev) vive en otro git (Gitea) y no se mezcla aquí. | El dueño |
| 2026-10-22 | «Pedir reel» gana campos (formato, prioridad, para cuándo, nivel, variantes, notas) y «remake» con los posts capturados por likes; el estado de un pedido se deriva del post que lo tomó, no se guarda dos veces. Lo genérico vive en guion-tablero (R-0013 AC11, AC12). Que un agente tome los pedidos solo queda para después. | El dueño («empieza así», sobre las opciones 1 a 3) |
