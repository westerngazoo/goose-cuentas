# goose-cuentas

La administración local de las cuentas de la casa: Rotor Físico,
fisicobuenfisico, PMMO y El extraño arte de computar. Corre en tu máquina,
lee los repos de las cuentas (en `~/code`) y guarda lo tuyo en tu vault personal.

| Capa | Dónde | Qué pone |
|---|---|---|
| Núcleo | guion-video-creator | Hace los videos y lee los posts (`guion-tablero`). Aquí se consume, no se copia. |
| Las cuentas | el repo de cada una | Su contenido, su marca y su `guion-cuentas.toml`. **El contenido se queda ahí.** |
| La administración | este repo y tu vault | El registro de las cuentas, las estadísticas que copias de Instagram, los pedidos. |

Nada de aquí publica en Instagram ni lee su API (la decisión OQ-4 sigue abierta).
Los números son los que copias tú, con la fecha en que los copiaste.

## Instalar

```bash
cd ~/code
git clone https://github.com/westerngazoo/goose-cuentas
cd goose-cuentas
cargo install --path .           # deja `cuentas` en ~/.cargo/bin
cuentas iniciar --vault "$HOME/vault/vault personal"   # una vez: crea cuentas/ y recuerda la ruta
```

`iniciar` crea, dentro del vault, `cuentas/` con su README, el registro
(`cuentas.toml`) y los dos CSV. Nunca pisa un archivo que ya existe. La ruta
queda en `~/.config/goose-cuentas/config.toml`. Si tu segundo cerebro ordena
las notas en otra carpeta (por ejemplo `Areas/Cuentas`), escribe ahí
`carpeta = "Areas/Cuentas"` antes de `iniciar`.

## Uso (las cifras son de ejemplo)

```bash
cuentas tablero                                  # cada cuenta: estados, seguidores, sus posts con más vistas
cuentas stats seguidores rotor 390               # los seguidores de hoy
cuentas stats post rotor deriva --vistas 1500 --likes 90 --guardados 12 --compartidos 4
cuentas stats post fbf 04-press --likes 2523     # un post viejo, sin carpeta
cuentas stats ver rotor
cuentas pedir pmmo "¿Por qué los cuaterniones son planos?"   # un pedido de reel
cuentas pedidos
cuentas publicado rotor rueda --enlace https://www.instagram.com/reel/…
cuentas nota                                     # reescribe cuentas/resumen.md
```

- `<cuenta>` es el `slug` del registro: `rotor`, `fbf`, `pmmo`, `computar`.
- `<post>` es el nombre de la carpeta del post en su repo. Para un post viejo
  sin carpeta, inventa un slug estable (`04-press`) y úsalo siempre igual.
- Una celda que no copiaste queda vacía: es «no se copió», no cero.
- Cada medición es una fila nueva. La historia es el dato: el tablero usa la
  más reciente de cada post.
- `pedir` agrega un encargo al `encargos.toml` del repo de la cuenta, el mismo
  que lee `guion tablero`. El agente de la cuenta lo toma desde ahí.
- `publicado` reescribe la línea `estado:` del README del post. El commit en el
  repo de la cuenta lo haces tú o el agente.

## En el vault

```text
cuentas/
  README.md                    qué es cada archivo y de dónde salen los números
  cuentas.toml                 el registro de las cuentas
  estadisticas/posts.csv       fecha,cuenta,post,vistas,alcance,likes,comentarios,guardados,compartidos,seguidores_ganados,nota
  estadisticas/seguidores.csv  fecha,cuenta,seguidores,nota
  resumen.md                   generado después de cada medición
```

Todo es texto: lo abres en Obsidian o en cualquier editor, y lo puedes
corregir a mano. El encabezado de cada CSV se compara letra por letra, así que
una columna movida falla con su nombre en vez de leerse mal.

## Lo que sigue

`docs/plan.md`. Lo próximo es la interfaz local (`cuentas ui`): ver las
estadísticas con gráficas y capturar likes y vistas en un formulario.
