# niri-animation-rotate

Un daemon liviano que rota las animaciones de ventanas de Niri al ocurrir eventos del compositor o mediante atajos de teclado manuales.

Se conecta al flujo de eventos IPC del compositor Niri y cicla a través de archivos KDL de animación. Soporta tanto modo automático (basado en eventos) como modo manual (activado mediante un atajo de teclado).

## Características

- **Modo automático** — rota al recibir eventos `WindowOpenedOrChanged`, `WindowClosed` y `WorkspaceActivated`
- **Filtros de eventos configurables** — excluye eventos específicos (`--no-window-opened`, `--no-window-closed`, `--no-workspace-activated`)
- **Modo manual** — rota bajo demanda mediante un socket de control Unix y un atajo de Niri
- **Orden aleatorio** — la secuencia de animaciones se mezcla aleatoriamente en cada inicio
- **Actualización automática** — vigila el directorio de animaciones en tiempo real para detectar archivos nuevos, eliminados o modificados
- **Enfriamiento** — tiempo mínimo opcional entre rotaciones para evitar cambios a mitad de reproducción
- **Modo sin recarga** — omite `niri msg action reload` para entornos que recargan la configuración automáticamente al cambiar archivos
- **Configuración KDL** — usa el mismo formato que Niri para la configuración
- **CLI + archivo de configuración** — configuración flexible mediante `--flags` o un archivo persistente
- **Escritura atómica** — escribe los archivos de animación de forma segura para evitar que Niri lea contenido parcial
- **Apagado graceful** — salida limpia al recibir SIGINT/SIGTERM
- **Depuración** — `--log-socket` para inspeccionar mensajes IPC crudos de Niri

## Prerrequisitos

- Una sesión activa del compositor [Niri](https://github.com/YaLTeR/niri)
- Rust toolchain (para compilar desde el código fuente)

## Instalación

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

### 1. Crear archivos de animación

Coloca tus archivos de animación `.kdl` en el directorio de animaciones:

```bash
mkdir -p ~/.config/niri/niri-animation-rotate/animations
```

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
| `--mode <MODO>` | Modo de operación: `auto` (eventos Niri) o `manual` (socket de control) | `auto` |
| `--control-socket <RUTA>` | Ruta del socket Unix para modo manual | `~/.config/niri/niri-animation-rotate/control.sock` |
| `--niri-socket <RUTA>` | Ruta del socket IPC de Niri (sobrescribe `$NIRI_SOCKET`) | `$NIRI_SOCKET` |
| `--cooldown-ms <MS>` | Milisegundos mínimos entre rotaciones (0 = sin enfriamiento) | `0` |
| `--no-reload` | Omite `niri msg action reload` después de rotar | — |
| `--no-window-opened` | No rotar al abrir/cambiar ventanas | — |
| `--no-window-closed` | No rotar al cerrar ventanas | — |
| `--no-workspace-activated` | No rotar al cambiar de espacio de trabajo | — |
| `--log-socket` | Imprime líneas IPC crudas de Niri a stderr (depuración) | — |
| `-h`, `--help` | Muestra la ayuda | — |
| `-V`, `--version` | Muestra la versión | — |

### Entorno

- `NIRI_SOCKET` — Ruta del socket IPC de Niri (establecida automáticamente por Niri en tu sesión). Se puede sobrescribir con `--niri-socket` o el archivo de configuración. No es necesaria en modo manual.
- `RUST_LOG` — Controla la verbosidad de los registros (por defecto: `info`)

### Modos

#### Modo automático (por defecto)

El daemon se conecta al flujo de eventos IPC de Niri y rota las animaciones automáticamente al recibir `WindowOpenedOrChanged`, `WindowClosed` y `WorkspaceActivated`. Este es el comportamiento por defecto.

```bash
niri-animation-rotate
```

Los primeros 5 eventos recibidos son estado inicial de Niri y se omiten.

#### Modo manual

En lugar de escuchar eventos de Niri, el daemon escucha en un socket Unix comandos `rotate`. Se usa junto con un atajo de teclado de Niri.

Primero, inicia el daemon en modo manual:

```bash
niri-animation-rotate --mode manual
```

Luego agrega un atajo a tu configuración de Niri (`~/.config/niri/config.kdl`):

```kdl
binds {
    Mod+Shift+A { spawn-sh "echo 'rotate' | nc -U $HOME/.config/niri/niri-animation-rotate/control.sock"; }
}
```

O con `socat`:

```kdl
binds {
    Mod+Shift+A { spawn-sh "echo 'rotate' | socat - UNIX-CONNECT:$HOME/.config/niri/niri-animation-rotate/control.sock"; }
}
```

El archivo del socket se limpia automáticamente al apagar.

> **Nota:** `spawn` de Niri no usa un shell y no expande `~` ni `$HOME`. Usa `spawn-sh` (Niri ≥ 25.08) o pasa la ruta absoluta completa con `spawn "sh" "-c" "..."`.

#### Enfriamiento

Para evitar cambios de animación a mitad de reproducción, establece un tiempo mínimo entre rotaciones:

```bash
niri-animation-rotate --cooldown-ms 3000
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
no-workspace-activated false
cooldown-ms 2000
mode "manual"
control-socket "~/.config/niri/niri-animation-rotate/control.sock"
```

Para las opciones booleanas (`log-socket`, `no-reload`, `no-window-opened`, `no-window-closed`, `no-workspace-activated`), el archivo de configuración solo puede habilitarlas. Para deshabilitar, omite la línea o usa el flag CLI.

### Prioridad de fusión

| Tipo de configuración | CLI | Archivo de configuración | Por defecto |
|---|---|---|---|
| Rutas (`animation-dir`, `animation-target`, `control-socket`) | `--ruta /x` gana | `ruta "/x"` | `~/.config/niri/...` |
| Socket Niri (`--niri-socket`) | `--niri-socket /x` gana | `niri-socket "/x"` | `$NIRI_SOCKET` |
| Booleanos (`log-socket`, `no-reload`, `no-window-opened`, `no-window-closed`, `no-workspace-activated`) | `--flag` gana (siempre activa) | `flag true` activa | `false` |
| Valores (`cooldown-ms`, `mode`) | `--valor X` gana | `valor X` aplica | `0` / `auto` |

## Cómo funciona

1. Al iniciar, escanea el directorio de animaciones en busca de archivos `.kdl`
2. Mezcla la lista de archivos aleatoriamente (la selección actual se preserva al reescanear el directorio)
3. **Preserva el archivo de salida existente** — no sobrescribe al iniciar
4. En modo automático: se conecta al flujo de eventos de Niri mediante un socket Unix
5. En modo manual: escucha en un socket de control comandos `rotate`
6. En cada activación de rotación, escribe el siguiente archivo de animación atómicamente y recarga la configuración de Niri
7. Vigila el directorio de animaciones para detectar cambios en el sistema de archivos y actualiza la caché automáticamente

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
