// Evita la consola extra en Windows en builds de release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    agentburn_lib::run()
}
