// No console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Started by the app or `localhost mcp` as the shared core (`targets`).
    if let [flag, storage_root] = &std::env::args_os().skip(1).collect::<Vec<_>>()[..]
        && flag == localhost_desktop::DAEMON_ARGUMENT
    {
        if let Err(message) = localhost_core::remote::shared::run(storage_root.as_ref()) {
            eprintln!("localhost: {message}");
            std::process::exit(1);
        }
        return;
    }
    localhost_desktop::run();
}
