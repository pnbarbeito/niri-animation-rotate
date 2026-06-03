# niri-animation-rotate

Un daemon liviano que rota las animaciones de ventanas de Niri al ocurrir eventos del compositor o mediante atajos de teclado manuales.

Se conecta al flujo de eventos IPC del compositor Niri y cicla a través de archivos KDL de animación. Soporta tanto modo automático (basado en eventos) como modo manual (activado mediante un atajo de teclado).

## Inicio rápido

De cero a animado en un minuto:

```bash
# Opción A — usa el script de instalación (recomendado)
git clone https://github.com/pnbarbeito/niri-animation-rotate.git
cd niri-animation-rotate
./install.sh

# Opción B — instalación manual
# cargo build --release
# cp target/release/niri-animation-rotate ~/.local/bin/
# mkdir -p ~/.config/niri/niri-animation-rotate/animations
# cp animations/*.kdl ~/.config/niri/niri-animation-rotate/animations/

# 1. Agrega a ~/.config/niri/config.kdl:
#    include "niri-animation-rotate/animation.kdl"

# 2. Ejecuta el daemon (dentro de tu sesión Niri):
niri-animation-rotate
```

¡Listo! 27 animaciones incluidas rotarán cada vez que abras o cierres una ventana.

## Características

- **Modo automático** — rota al recibir eventos `WindowOpenedOrChanged` y `WindowClosed`
- **Filtros de eventos configurables** — excluye eventos específicos (`--no-window-opened`, `--no-window-closed`)
- **Enfriamiento inteligente** — cooldown que analiza la duración real de cada animación para evitar cambios a mitad de reproducción
- **Modo manual** — rota bajo demanda mediante un socket de control Unix con respuestas bidireccionales
- **Comandos completos** — `next`, `prev`, `current`, `list` y `select <nombre>` para control manual
- **Mezcla configurable** — mezcla aleatoriamente el orden al iniciar (`--random-order`, desactivada por defecto)
- **Actualización automática** — vigila el directorio de animaciones en tiempo real para detectar archivos nuevos, eliminados o modificados
- **27 animaciones incluidas** — presets listos para usar en el repositorio
- **Modo sin recarga** — omite `niri msg action reload` para entornos que recargan la configuración automáticamente al cambiar archivos
- **Configuración KDL** — usa el mismo formato que Niri para la configuración
- **CLI + archivo de configuración** — configuración flexible mediante `--flags` o un archivo persistente
- **Escritura atómica** — escribe los archivos de animación de forma segura para evitar que Niri lea contenido parcial
- **Apagado graceful** — salida limpia al recibir SIGINT/SIGTERM
- **Depuración** — `--log-socket` para inspeccionar mensajes IPC crudos de Niri
- **CLI nrctl** — control rápido por terminal para modo manual (`nrctl next`, `nrctl select <nombre>`, etc.)

## Prerrequisitos

- Una sesión activa del compositor [Niri](https://github.com/YaLTeR/niri)
- Rust toolchain (para compilar desde el código fuente)

## Instalación

### Automática (script de instalación)

La forma más fácil de empezar:

```bash
git clone https://github.com/pnbarbeito/niri-animation-rotate.git
cd niri-animation-rotate
./install.sh
```

Este script:

1. Verifica que Rust/Cargo y Git estén instalados
2. Compila el binario
3. Lo copia a `~/.local/bin/`
4. Crea la estructura de directorios de configuración
5. Copia las 27 animaciones incluidas a `~/.config/niri/niri-animation-rotate/animations/`
6. Instala el script de control `nrctl` en `~/.local/bin/`
7. Muestra los siguientes pasos para configurar Niri

Para instalar también un servicio systemd, añade `--systemd`:

```bash
./install.sh --systemd
```

Para más detalles, consulta `./install.sh --help`.

### Desde el código fuente

```bash
git clone https://github.com/pnbarbeito/niri-animation-rotate.git
cd niri-animation-rotate
cargo build --release
```

El binario estará en `target/release/niri-animation-rotate`.

### Copiar a ~/.local/bin (recomendado)

Copia el binario a un directorio en tu `PATH` para poder ejecutarlo desde cualquier lugar:

```bash
cp target/release/niri-animation-rotate ~/.local/bin/
```

Esto es especialmente útil si planeas ejecutarlo como un servicio systemd (ver más abajo).

### Mediante cargo (alternativa)

Si prefieres, puedes instalarlo directamente desde el repositorio:

```bash
cargo install --git https://github.com/pnbarbeito/niri-animation-rotate.git
```

El binario estará en `~/.cargo/bin/niri-animation-rotate`.

## Configuración

### 1. Archivos de animación

El repositorio incluye **27 presets de animación listos para usar** en la carpeta `animations/`. Después de compilar, cópialos al directorio de configuración:

```bash
mkdir -p ~/.config/niri/niri-animation-rotate/animations
cp animations/*.kdl ~/.config/niri/niri-animation-rotate/animations/
```

También puedes agregar tus propios archivos `.kdl` aquí — el daemon los detecta automáticamente en tiempo real.

Cada archivo `.kdl` debe contener un bloque `animations { ... }` completo de Niri. Por ejemplo:

```kdl
// ~/.config/niri/niri-animation-rotate/animations/spring-bouncy.kdl
animations {
    workspace-switch {
        spring damping-ratio=0.8 stiffness=1000 epsilon=0.0001
    }
    window-open {
        duration-ms 200
        curve "ease-out-expo"
    }
    window-close {
        duration-ms 150
        curve "ease-out-quad"
    }
}
```

### 2. Configurar Niri para incluir el archivo de animación

Agrega esta línea a tu configuración principal de Niri (`~/.config/niri/config.kdl`):

```kdl
include "niri-animation-rotate/animation.kdl"
```

### 3. Crear un archivo de configuración (opcional)

Consulta la [sección de configuración](#configuración-1) más abajo para todas las opciones disponibles.

### 4. Ejecutar el daemon

```bash
niri-animation-rotate
```

---

## Uso

```
niri-animation-rotate [OPCIONES]
```

### Opciones

| Flag | Descripción | Por defecto |
|---|---|---|
| `--config <RUTA>` | Ruta al archivo de configuración (formato KDL) | `~/.config/niri/niri-animation-rotate/config.kdl` |
| `--animation-dir <DIR>` | Directorio con archivos `.kdl` de animación | `~/.config/niri/niri-animation-rotate/animations` |
| `--animation-target <RUTA>` | Archivo de salida que Niri lee mediante `include` | `~/.config/niri/niri-animation-rotate/animation.kdl` |
| `--mode <MODO>` | Modo de operación: `auto` (eventos Niri) o `manual` (socket con `next`/`prev`/`select`/`current`/`list`) | `auto` |
| `--control-socket <RUTA>` | Ruta del socket Unix para modo manual | `~/.config/niri/niri-animation-rotate/control.sock` |
| `--random-order` | Mezcla aleatoriamente el orden al iniciar y al reescanear (desactivado por defecto) | — |
| `--niri-socket <RUTA>` | Ruta del socket IPC de Niri (sobrescribe `$NIRI_SOCKET`) | `$NIRI_SOCKET` |
| `--cooldown-ms <MS>` | Buffer extra añadido sobre la duración de animación analizada (ms) | `0` |
| `--duration-fallback-ms <MS>` | Duración por defecto para animaciones sin `duration-ms` | `500` |
| `--no-reload` | Omite `niri msg action reload` después de rotar | — |
| `--no-window-opened` | No rotar al abrir/cambiar ventanas | — |
| `--no-window-closed` | No rotar al cerrar ventanas | — |
| `--log-socket` | Imprime líneas IPC crudas de Niri a stderr (depuración) | — |
| `-h`, `--help` | Muestra la ayuda | — |
| `-V`, `--version` | Muestra la versión | — |

### Entorno

- `NIRI_SOCKET` — Ruta del socket IPC de Niri (establecida automáticamente por Niri en tu sesión). Se puede sobrescribir con `--niri-socket` o el archivo de configuración. No es necesaria en modo manual.
- `RUST_LOG` — Controla la verbosidad de los registros (por defecto: `info`)

### Modos

#### Modo automático (por defecto)

El daemon se conecta al flujo de eventos IPC de Niri y rota las animaciones automáticamente al recibir `WindowOpenedOrChanged` y `WindowClosed`. Este es el comportamiento por defecto.

```bash
niri-animation-rotate
```

Los primeros 5 eventos recibidos son estado inicial de Niri y se omiten.

#### Modo manual

En lugar de escuchar eventos de Niri, el daemon escucha en un socket Unix comandos de rotación. Se usa junto con atajos de teclado de Niri.

Primero, inicia el daemon en modo manual:

```bash
niri-animation-rotate --mode manual
```

Luego agrega atajos a tu configuración de Niri (`~/.config/niri/config.kdl`):

```kdl
binds {
    Mod+Shift+A { spawn-sh "echo 'next' | nc -U $HOME/.config/niri/niri-animation-rotate/control.sock"; }
    Mod+Shift+D { spawn-sh "echo 'prev' | nc -U $HOME/.config/niri/niri-animation-rotate/control.sock"; }
}
```

**Comandos disponibles:**

| Comando | Respuesta | Descripción |
|---------|-----------|-------------|
| `next` | _(ninguna)_ | Avanza a la siguiente animación |
| `prev` | _(ninguna)_ | Retrocede a la animación anterior |
| `rotate` | _(ninguna)_ | Alias de `next` (compatibilidad con versiones anteriores) |
| `current` | `prism_fold` | Devuelve el nombre del archivo (sin `.kdl`) de la animación activa |
| `list` | `bloom`\n`prism_fold`\n`tv_crt`\n | Devuelve todas las animaciones disponibles, una por línea |
| `select <nombre>` | `ok` o `error: not found` | Selecciona una animación específica por nombre (sin `.kdl`, sin distinguir mayúsculas) |
| `mode auto` | `ok` | Cambia a modo automático (eventos Niri). Reinicia el temporizador de la animación actual |
| `mode manual` | `ok` | Cambia a modo manual (socket de control). El enfriamiento manual comienza de cero |

Todas las respuestas son texto plano, una línea cada una (múltiples líneas para `list`). Los comandos desconocidos devuelven `error: unknown command: <cmd>`.

> **Cambio en caliente:** Puedes cambiar de modo en cualquier momento mediante el socket de control. Inicia en modo automático, envía `mode manual` para tomar control manual, y `mode auto` para reanudar la rotación automática. El socket de control (consultas `current`/`list`) funciona en ambos modos.

**Ejemplos con `nc` (netcat):**

> **Consejo:** Para el control diario, usa `nrctl` en lugar de `nc` crudo — es más simple y muestra el resultado después de cada rotación. Consulta la [sección de nrctl](#nrctl--herramienta-de-control-por-terminal).

```bash
# Obtener el nombre de la animación actual
echo "current" | nc -U ~/.config/niri/niri-animation-rotate/control.sock
# → prism_fold

# Listar todas las animaciones disponibles
echo "list" | nc -U ~/.config/niri/niri-animation-rotate/control.sock
# → bloom
# → prism_fold
# → tv_crt

# Seleccionar una animación específica
echo "select tv_crt" | nc -U ~/.config/niri/niri-animation-rotate/control.sock
# → ok

echo "select inexistente" | nc -U ~/.config/niri/niri-animation-rotate/control.sock
# → error: not found
```

El archivo del socket se limpia automáticamente al apagar.

> **Nota:** `spawn` de Niri no usa un shell y no expande `~` ni `$HOME`. Usa `spawn-sh` (Niri ≥ 25.08) o pasa la ruta absoluta completa con `spawn "sh" "-c" "..."`.

#### Enfriamiento (según duración)

El daemon lee el valor `duration-ms` de cada archivo KDL de animación y lo usa para calcular cuándo terminará de reproducirse la animación actual. Las rotaciones se bloquean mientras la animación actual sigue activa.

```bash
# Sin buffer extra — rota solo cuando la animación actual termina
niri-animation-rotate --cooldown-ms 0

# Añade un buffer de 1000ms después de que termine la animación
niri-animation-rotate --cooldown-ms 1000
```

**Cómo funciona:**

- Al iniciar, el daemon analiza la animación activa para determinar cuánto tiempo le queda de reproducción
- Cada evento entrante reinicia el temporizador de bloqueo usando la duración de la animación actual — no ocurre rotación hasta que expire
- Después de una rotación, el temporizador usa la duración de la **nueva** animación
- Si un archivo no tiene `duration-ms`, se usa `--duration-fallback-ms` (por defecto: 500ms)
- Si la animación tiene un multiplicador `slowdown <float>` (ej. `slowdown 1.5`), se aplica automáticamente
- En **modo manual**, el enfriamiento es fijo (no se analizan las duraciones)

### nrctl — herramienta de control por terminal

`nrctl` es un script de conveniencia instalado junto al demonio que te permite controlarlo desde la terminal sin escribir comandos `nc -U` crudos.

Si el demonio está ejecutándose:

```bash
nrctl next                       # → rota a la siguiente animación, muestra resultado
nrctl prev                       # ← retrocede a la animación anterior
nrctl current                    # muestra el nombre de la animación activa
nrctl list                       # lista todas las animaciones disponibles
nrctl list --json                # lista como arreglo JSON
nrctl select <nombre>            # selecciona una animación específica (sin distinguir mayúsculas)
nrctl status                     # muestra el estado del demonio (modo, cooldown, etc.)
nrctl status --json              # estado como JSON
nrctl mode auto                  # cambia a modo automático
nrctl mode manual                # cambia a modo manual

nrctl --help                     # referencia completa de comandos
```

#### `nrctl set` — configuración en caliente

Todos los cambios con `set` se persisten al archivo de configuración (`~/.config/niri/niri-animation-rotate/config.kdl`).

| Clave | Valores | Descripción |
|-------|---------|-------------|
| `cooldown-ms` | milisegundos (ej. `500`, `2000`, `0`) | Tiempo mínimo entre rotaciones |
| `random-order` | `true`, `false`, `1`, `0`, `yes`, `no` | Mezclar orden de animaciones al iniciar/reescanear |
| `no-window-opened` | `true`, `false`, `1`, `0`, `yes`, `no` | Ignorar eventos de apertura/cambio de ventana |
| `no-window-closed` | `true`, `false`, `1`, `0`, `yes`, `no` | Ignorar eventos de cierre de ventana |

Ejemplos:

```bash
nrctl set cooldown-ms 2000       # cooldown de 2 segundos
nrctl set cooldown-ms 0          # desactivar cooldown
nrctl set random-order true      # activar mezcla aleatoria
nrctl set random-order yes       # equivalente (alias booleano)
nrctl set no-window-opened 1     # ignorar eventos de ventana
nrctl set no-window-closed no    # re-activar eventos de cierre
```

Define `NRCTL_SOCKET` para sobrescribir la ruta del socket por defecto:

```bash
NRCTL_SOCKET=/tmp/custom.sock nrctl current
```

## Configuración

La aplicación usa un sistema de configuración de tres niveles (prioridad más alta gana):

1. **Argumentos CLI** (prioridad más alta)
2. **Archivo de configuración** (formato KDL)
3. **Valores por defecto** (prioridad más baja)

### Formato del archivo de configuración

El archivo de configuración (`~/.config/niri/niri-animation-rotate/config.kdl`) soporta todas las opciones CLI:

```kdl
animation-dir "~/.config/niri/niri-animation-rotate/animations"
animation-target "~/.config/niri/niri-animation-rotate/animation.kdl"
niri-socket "/run/user/1000/niri.sock"
log-socket true
no-reload true
no-window-opened false
no-window-closed false
cooldown-ms 2000
duration-fallback-ms 500
random-order true
mode "manual"
control-socket "~/.config/niri/niri-animation-rotate/control.sock"
```

Para las opciones booleanas (`log-socket`, `no-reload`, `no-window-opened`, `no-window-closed`), el archivo de configuración solo puede habilitarlas. Para deshabilitar, omite la línea o usa el flag CLI.

### Prioridad de fusión

| Tipo de configuración | CLI | Archivo de configuración | Por defecto |
|---|---|---|---|
| Rutas (`animation-dir`, `animation-target`, `control-socket`) | `--ruta /x` gana | `ruta "/x"` | `~/.config/niri/...` |
| Socket Niri (`--niri-socket`) | `--niri-socket /x` gana | `niri-socket "/x"` | `$NIRI_SOCKET` |
| Booleanos (`log-socket`, `no-reload`, `no-window-opened`, `no-window-closed`, `random-order`) | `--flag` gana (siempre activa) | `flag true` activa | `false` |
| Valores (`cooldown-ms`, `duration-fallback-ms`, `mode`) | `--valor X` gana | `valor X` aplica | `0` / `500` / `auto` |

## Cómo funciona

1. Al iniciar, escanea el directorio de animaciones en busca de archivos `.kdl`
2. Analiza la duración de cada animación a partir de `duration-ms` y el multiplicador `slowdown`
3. Lee la animación activa para establecer el temporizador de enfriamiento inicial
4. Opcionalmente mezcla la lista (si `--random-order` está activado); la selección actual se preserva al reescanear
5. **Preserva el archivo de salida existente** — no sobrescribe al iniciar
6. En modo automático: se conecta al flujo de eventos de Niri mediante un socket Unix
7. En cada evento `WindowOpenedOrChanged` o `WindowClosed`, verifica el temporizador:
    - Si la animación actual terminó → rota
    - Si la animación sigue reproduciéndose → extiende el bloqueo (sin rotación)
8. En modo manual: escucha en un socket de control comandos `next`/`prev`/`select`/`current`/`list` (enfriamiento fijo)
9. En cada activación de rotación, escribe el siguiente archivo de animación atómicamente y recarga la configuración de Niri
10. Vigila el directorio de animaciones para detectar cambios en el sistema de archivos y actualiza la caché automáticamente

## Registros

Los registros se escriben en stderr. Controla la verbosidad con `RUST_LOG`:

```bash
# Por defecto (nivel info)
niri-animation-rotate

# Salida de depuración
RUST_LOG=debug niri-animation-rotate

# Traza (muy verboso)
RUST_LOG=trace niri-animation-rotate
```

## Ejecutar como servicio systemd (opcional)

Crea `~/.config/systemd/user/niri-animation-rotate.service`.

Ajusta `ExecStart` según dónde hayas colocado el binario:
- Si lo copiaste a `~/.local/bin/` (recomendado), usa la ruta de abajo
- Si usaste `cargo install --git`, el binario está en `~/.cargo/bin/`
- Si compilaste desde el código fuente sin copiarlo, apunta a `target/release/niri-animation-rotate`

```ini
[Unit]
Description=Niri Animation Rotate
After=niri-session.service

[Service]
Type=simple
ExecStart=%h/.local/bin/niri-animation-rotate
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

Para modo manual, añade el flag `--mode manual`:

```
ExecStart=%h/.local/bin/niri-animation-rotate --mode manual
```

Activar e iniciar:

```bash
systemctl --user daemon-reload
systemctl --user enable niri-animation-rotate
systemctl --user start niri-animation-rotate
```

## Licencia

MIT