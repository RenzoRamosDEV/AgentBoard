# Guía de contribución

AgentBoard es un proyecto de código abierto. Las contribuciones —correcciones, mejoras,
soporte para nuevos agentes o traducciones— son bienvenidas. Este documento resume cómo
está montado el proyecto y qué se espera de una contribución.

## Filosofía del proyecto

- **Solo lectura y sin persistencia.** AgentBoard lee los logs que los agentes ya dejan en el
  ordenador y los agrega en una base SQLite **en memoria**. No escribe datos de uso en disco
  (la única excepción es el archivo de ajustes). Cualquier cambio debe respetar esa premisa.
- **Local y privado.** Nada sale de la máquina: sin proxy, sin claves de API, sin telemetría.
- **Especificación primero.** Los cambios se describen con [OpenSpec](https://github.com/Fission-AI/OpenSpec)
  antes de implementarse (ver `openspec/`).

## Entorno de desarrollo

Requisitos: Node.js 20+, Rust estable (rustup) y las dependencias de sistema de Tauri
(en Linux, WebKitGTK 4.1). El desarrollo se hace cómodo en un contenedor
(por ejemplo `distrobox`/`toolbox`) con esas dependencias.

```bash
npm ci                      # dependencias del frontend
npm run tauri dev           # levanta la app (Vite + Tauri) con recarga en caliente
```

Comprobaciones antes de abrir un PR:

```bash
# Frontend
npx tsc -b                  # tipos
npm test                    # vitest

# Backend (Rust)
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --bins -- -D warnings
cargo test  --manifest-path src-tauri/Cargo.toml
```

El servidor MCP se compila y prueba aparte:

```bash
cargo build --manifest-path src-tauri/Cargo.toml --bin agentboard-mcp
cargo test  --manifest-path src-tauri/Cargo.toml --bin agentboard-mcp
```

## Estructura

```
src/                  UI React + TypeScript (views/, components/, lib/)
src/assets/           todas las imágenes (icono de la app y logos del panel)
src-tauri/src/
  providers/          un módulo por agente (trait Provider)
  ingest.rs           lectura incremental por offsets, sin duplicar
  queries.rs          consultas agregadas del dashboard
  pricing.rs          precios por modelo (USD / millón de tokens)
  watcher.rs          vigilante de archivos en vivo
src-tauri/mcp/        crate del servidor MCP (stdio, JSON-RPC)
openspec/             propuestas y especificaciones
```

## Cómo se añade soporte para un agente nuevo

1. Crear un módulo en `src-tauri/src/providers/` que implemente el trait `Provider`
   (detección, rutas de logs y parseo de sus registros a los tipos comunes).
2. Registrarlo en `providers::all()`.
3. Añadir un fixture anonimizado en `tests/fixtures/<agente>/` y un test de ingesta.
4. Normalizar sus nombres de herramienta/modelo a los de Claude Code cuando aplique.

## Estilo de commits

- En **español**, siguiendo [Conventional Commits](https://www.conventionalcommits.org/)
  (`feat:`, `fix:`, `docs:`, `refactor:`, `ci:`…).
- **Un commit por cambio lógico**, no todo junto.

## Traducciones

Los textos viven en `src/lib/i18n.ts`, con el **español como clave**. Para añadir o corregir
un idioma (en, pt, fr) basta con completar su diccionario respetando las mismas claves y las
variables `{n}`.

## Integración continua

Cada Pull Request compila y prueba en **Linux, Windows y macOS** (matriz en `.github/workflows/ci.yml`).
Un PR debe quedar en verde en los tres antes de fusionarse.
