use tauri::{Manager, Window};

#[tauri::command]
fn start_wallet_connection(window: Window, message: String) -> String {
    println!(
        "I was invoked from JS, with this message: {}",
        message
    );

    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(5));
        window.emit("wallet_connection", Payload { message: "Tauri is awesome!".into() }).unwrap();
    });

    "Hello from Rust!".into()

}

// the payload type must implement `Serialize` and `Clone`.
#[derive(Clone, serde::Serialize)]
struct Payload {
    message: String,
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_oauth::init())
        .invoke_handler(tauri::generate_handler![start_wallet_connection])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
