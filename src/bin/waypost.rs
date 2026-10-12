use std::{net::SocketAddr, path::PathBuf, rc::Rc};
use waypost::{Config, native::server};
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1)
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() || args.iter().any(|s| s == "--help" || s == "-h") {
        println!(
            "Waypost\n\n  waypost init [--data ./var]\n  waypost encrypt [--data ./var]\n  waypost serve [--data ./var] [--listen 127.0.0.1:8787]\n\nWAYPOST_ADMIN_SECRET overrides the private admin-secret file.\nWAYPOST_ENCRYPTION_KEYS sets the private JSON encryption keyring.\nWAYPOST_CURSOR_SECRET optionally separates cursor signing from admin rotation.\nWAYPOST_PUBLIC_ORIGIN sets the browser Origin / generated HTTP URLs."
        );
        return Ok(());
    }
    let command = &args[0];
    if !["init", "serve", "encrypt"].contains(&command.as_str()) {
        return Err("Unknown command; use --help.".into());
    }
    let mut data = PathBuf::from("var");
    let mut address: SocketAddr = "127.0.0.1:8787".parse()?;
    let mut i = 1;
    while i < args.len() {
        let v = args.get(i + 1).ok_or("Missing option value.")?;
        match args[i].as_str() {
            "--data" => data = PathBuf::from(v),
            "--listen" => address = v.parse()?,
            _ => return Err("Unknown option.".into()),
        }
        i += 2;
    }
    std::fs::create_dir_all(&data)?;
    let secret = match std::env::var("WAYPOST_ADMIN_SECRET") {
        Ok(value) => value,
        Err(_) => {
            let path = data.join("admin-secret");
            if !path.exists() {
                use std::io::Write;
                let mut bytes = [0; 32];
                getrandom::getrandom(&mut bytes).map_err(|_| "Unable to generate credential.")?;
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                match options.open(&path) {
                    Ok(mut file) => {
                        file.write_all(hex::encode(bytes).as_bytes())?;
                        file.sync_all()?
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(e) => return Err(e.into()),
                }
            }
            std::fs::read_to_string(path)?.trim().to_owned()
        }
    };
    let _runtime_lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(data.join("runtime-lock"))?;
    _runtime_lock.try_lock().map_err(
        |_| "This data directory is in use; stop the service before initialization or migration.",
    )?;
    let origin =
        std::env::var("WAYPOST_PUBLIC_ORIGIN").unwrap_or_else(|_| format!("http://{address}"));
    let keyring = match std::env::var("WAYPOST_ENCRYPTION_KEYS") {
        Ok(value) => zeroize::Zeroizing::new(value),
        Err(_) => {
            let path = data.join("encryption-keys");
            if !path.exists() {
                if data.join("metadata.sqlite").exists() {
                    return Err("Encryption keyring is missing. Restore encryption-keys, or explicitly configure WAYPOST_ENCRYPTION_KEYS for legacy migration.".into());
                }
                use std::io::Write;
                let mut bytes = zeroize::Zeroizing::new([0u8; 32]);
                getrandom::getrandom(bytes.as_mut())
                    .map_err(|_| "Unable to generate encryption key.")?;
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                match options.open(&path) {
                    Ok(mut file) => {
                        file.write_all(serde_json::json!({"active":"v1","keys":{"v1":hex::encode(bytes.as_ref())}}).to_string().as_bytes())?;
                        file.sync_all()?;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(e) => return Err(e.into()),
                }
            }
            zeroize::Zeroizing::new(std::fs::read_to_string(path)?)
        }
    };
    let migration_admin = secret.clone();
    let app = server::open_app(
        &data,
        Config {
            admin_secret: secret,
            public_origin: origin.clone(),
        },
    )?;
    let app = if let Ok(secret) = std::env::var("WAYPOST_CURSOR_SECRET") {
        app.with_cursor_secret(&secret)?
    } else {
        app
    };
    let allow_legacy = std::env::var("WAYPOST_ALLOW_LEGACY_PLAINTEXT").is_ok_and(|v| v == "1");
    let app = app.with_encryption_keys(&keyring, allow_legacy)?;
    let app = Rc::new(app);
    if command == "init" {
        println!(
            "Initialized {}. Admin credential: {}/admin-secret (or WAYPOST_ADMIN_SECRET).",
            data.display(),
            data.display()
        );
        return Ok(());
    }
    if command == "encrypt" {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(async {
            for kind in ["objects", "messages", "uploads", "blobs", "memory_items"] {
                let mut cursor = serde_json::Value::Null;
                loop {
                    let response = app
                        .handle(waypost::Request::json(
                            "POST",
                            "/admin/v0/maintenance/encrypt",
                            &migration_admin,
                            serde_json::json!({"kind":kind,"dry_run":false,"cursor":cursor}),
                        ))
                        .await;
                    let value: serde_json::Value = serde_json::from_slice(&response.body)
                        .map_err(|_| "Invalid migration response.")?;
                    if response.status != 200 {
                        return Err(
                            "Encryption migration failed; retry with the same keyring.".into()
                        );
                    }
                    if value["data"]["has_more"] != true {
                        break;
                    }
                    cursor = value["data"]["next_cursor"].clone();
                }
                println!("Encrypted {kind}.");
            }
            Ok::<(), Box<dyn std::error::Error>>(())
        })?;
        return Ok(());
    }
    eprintln!("Waypost listening at {address}; MCP: {origin}/mcp");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    tokio::task::LocalSet::new().block_on(&runtime, server::listen(address, app))?;
    Ok(())
}
