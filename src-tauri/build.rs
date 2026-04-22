use std::path::Path;

/// Environment variables to burn into the binary at compile time.
/// (name, default_value)
///
/// API_BASE_URL, BITCOIN_NETWORK, and OAUTH2_AUTH_URL all live in the
/// runtime environment manifest (fetched from
/// sigvault.org/environments.json) because they are per-deployment.
/// The OAuth client_id and token_url are shared across all envs and so
/// remain compile-time.
const BUILD_ENV_VARS: &[(&str, &str)] = &[
    ("OAUTH2_CLIENT_ID", "346819126007796376"),
    (
        "OAUTH2_TOKEN_URL",
        "https://sigvault-jsyfl0.us1.zitadel.cloud/oauth/v2/token",
    ),
];

fn main() {
    // Load .env from the desktop app root (parent of src-tauri/)
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let env_path = Path::new(&manifest_dir).join("../.env");

    if env_path.exists() {
        dotenvy::from_path(&env_path).ok();
    }

    // Emit each variable as a compile-time env var.
    // Priority: process env (which includes .env via dotenvy) > default
    for (key, default) in BUILD_ENV_VARS {
        let value = std::env::var(key).unwrap_or_else(|_| default.to_string());
        println!("cargo:rustc-env={key}={value}");
    }

    // Re-run build script if .env changes
    println!("cargo:rerun-if-changed=../.env");

    tauri_build::build()
}
