// Evita la consola extra en Windows en builds de release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // WebKitGTK puede dejar la ventana en blanco con el renderer DMABUF en algunos GPU (sobre todo
    // NVIDIA). Se desactiva en el propio proceso, antes de iniciar Tauri, salvo que el usuario ya
    // lo haya fijado. (El EGL_BAD_PARAMETER del AppImage en Mesa moderno se arregla aparte, en el
    // empaquetado: ver scripts/fix-appimage.sh.)
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    agentboard_lib::run()
}
