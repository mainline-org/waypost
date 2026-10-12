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
            "Waypost\n\n  waypost init [--data ./var]\n  waypost serve [--data ./var] [--listen 127.0.0.1:8787]\n\nWAYPOST_ADMIN_SECRET overrides the private admin-secret file.\nWAYPOST_PUBLIC_ORIGIN sets the browser Origin / generated HTTP URLs."
        );
        return Ok(());
    }
    let command = &args[0];
    if !["init", "serve"].contains(&command.as_str()) {
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
    let origin =
        std::env::var("WAYPOST_PUBLIC_ORIGIN").unwrap_or_else(|_| format!("http://{address}"));
    let app = Rc::new(server::open_app(
        &data,
        Config {
            admin_secret: secret,
            public_origin: origin.clone(),
        },
    )?);
    if command == "init" {
        println!(
            "Initialized {}. Admin credential: {}/admin-secret (or WAYPOST_ADMIN_SECRET).",
            data.display(),
            data.display()
        );
        return Ok(());
    }
    eprintln!("Waypost listening at {address}; MCP: {origin}/mcp");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    tokio::task::LocalSet::new().block_on(&runtime, server::listen(address, app))?;
    Ok(())
}
