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
cd ~/projects                    # o ~/code: donde tengas tus repos
git clone https://github.com/westerngazoo/goose-cuentas
cd goose-cuentas
cargo install --path .           # deja `cuentas` en ~/.cargo/bin
cuentas iniciar
```

`iniciar` busca tu vault personal: en `~/vaults`, en la lista de vaults de
Obsidian y en cualquier carpeta con `.obsidian/`. Si encuentra varios, te los
lista y eliges uno con su número; la elección se guarda en
`~/.config/goose-cuentas/config.toml`. También puedes nombrarlo directo:
`cuentas iniciar --vault "<ruta>"`. En zsh no pongas comentarios con `#` en la
misma línea; si lo haces, `cuentas` los ignora.

Las cuatro cuentas viven en un solo repo, `rotorf-sico` (fbf en `rotorf-sico/fbf/`).
Se busca donde dice `cuentas/cuentas.toml`; si no está ahí, en `~/code`,
`~/projects` y carpetas parecidas, con el mismo nombre. Si una cuenta
se muda de repo (como fbf el 2026-10-21), el registro se corrige solo la
siguiente vez que se lee: una cuenta cuyo repo ya no existe se apunta al repo
del registro que la declare, y se reescribe esa línea en `cuentas.toml`.

`iniciar` crea, dentro del vault, `cuentas/` con su README, el registro
(`cuentas.toml`) y los dos CSV. Nunca pisa un archivo que ya existe. La ruta
queda en `~/.config/goose-cuentas/config.toml`. Si tu segundo cerebro ordena
las notas en otra carpeta (por ejemplo `Areas/Cuentas`), escribe ahí
`carpeta = "Areas/Cuentas"` antes de `iniciar`.

## La interfaz

```bash
cuentas ui                 # abre http://127.0.0.1:7171 en tu navegador
```

Solo se abre en tu propia máquina (127.0.0.1). Tiene cinco pestañas:

- **Publicar** (la que abre): lo que dice `publicar/ORDEN.md`, cuenta por cuenta y en orden. Cada reel trae su video para verlo ahí mismo, su caption y su texto alternativo con un botón para copiarlos, la ruta local del video y de la portada (con «Copiar ruta», para pegarla en el diálogo de subir, y «Mostrar en Finder»), los botones para bajarlos, y «Ya lo subí» con el enlace. Ese botón marca la entrada en `ORDEN.md` y el README del post. El siguiente de cada cuenta va resaltado.
- **Tablero:** cada cuenta con sus seguidores, vistas, likes e interacción; una gráfica de seguidores en el tiempo y otra de vistas por post.
- **Capturar:** las cifras de un post (vistas, alcance, likes, comentarios, guardados, compartidos, seguidores ganados) y los seguidores de una cuenta.
- **Pedir reel:** la cuenta, el formato y la pregunta.
- **Posts:** el estado de cada post y el botón «Ya lo subí».

Escribe los mismos archivos que la línea de comandos. Se cierra con Ctrl-C.

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

`docs/plan.md`.
