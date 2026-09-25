// Evita la consola extra en Windows en builds de release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // En algunos GPU/compositores de Wayland (p. ej. sistemas atómicos como Bazzite/Silverblue)
    // el renderer DMABUF de WebKitGTK no puede crear el display EGL y la ventana no arranca
    // («Could not create default EGL display: EGL_BAD_PARAMETER. Aborting...»). Desactivarlo lo
    // evita. Se fija en el propio proceso, antes de iniciar Tauri, para que funcione tras la
    // instalación sin depender del entorno del lanzador; solo si el usuario no lo ha fijado.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    agentboard_lib::run()
}
