//! Standalone native SDK exercise. No Tauri or Go process is required.
use cybergrid_sdk::{
    AuthFuture, AuthProvider, CancellationToken, Client, CloudflaredConfig, Config,
    CredentialStore, Port,
};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
// Explicit test-only provider. The argument is accepted only for HTTP loopback;
// production hosts supply their own OAuth AuthProvider / CredentialStore.
struct ExplicitLoopbackFile(std::path::PathBuf);
impl AuthProvider for ExplicitLoopbackFile {
    fn access_token<'a>(&'a self, _: &'a Client) -> AuthFuture<'a> {
        Box::pin(async move {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            let mut file = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&self.0)?;
            if !file.metadata()?.is_file() || file.metadata()?.permissions().mode() & 0o077 != 0 {
                return Err(cybergrid_sdk::Error::Credentials(
                    "harness token file must be private".into(),
                ));
            }
            let mut token = String::new();
            use std::io::Read;
            Read::by_ref(&mut file)
                .take(16385)
                .read_to_string(&mut token)?;
            if token.len() > 16384 {
                return Err(cybergrid_sdk::Error::Credentials(
                    "harness token file too large".into(),
                ));
            }
            Ok(token.trim().to_owned())
        })
    }
}
fn args() -> (String, HashMap<String, String>) {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".into());
    let mut out = HashMap::new();
    while let Some(k) = args.next() {
        if let Some(v) = args.next() {
            out.insert(k.trim_start_matches('-').to_owned(), v);
        }
    }
    (command, out)
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (command, args) = args();
    if command == "version" {
        println!("{}", cybergrid_sdk::VERSION);
        return Ok(());
    }
    if command == "help" {
        println!("lab login|host|devices|probe|logout --app LOGTO_NATIVE_ID --credentials PRIVATE_FILE [--server HTTPS_ROOT] [--peer DEVICE_ID] [--bytes 65536] [--provider-cache ABSOLUTE_DIR] [--native-library-dir ANDROID_INSTALLED_DIR] [--cloudflared EXPLICIT_EXTERNAL_PATH] [--no-tunnel true] [--direct-ms 12000] [--interface en0]\nTest harness only: --access-token-file PRIVATE_FILE permits explicit loopback server API exchange.");
        return Ok(());
    }
    let app = args
        .get("app")
        .cloned()
        .or_else(|| std::env::var("CYBERGRID_APP_ID").ok())
        .ok_or("--app or CYBERGRID_APP_ID required")?;
    let mut cfg = Config {
        app_id: app,
        ..Config::default()
    };
    if let Some(s) = args.get("server") {
        cfg.base_url = s.clone();
    }
    if let Some(ms) = args.get("direct-ms") {
        cfg.direct_timeout = Duration::from_millis(ms.parse()?);
    }
    if args.get("stun").is_some_and(|v| v == "none") {
        cfg.stun_servers.clear();
    }
    cfg.interface_name = args.get("interface").cloned();
    cfg.cloudflared = if args.get("no-tunnel").is_some_and(|v| v == "true") {
        None
    } else if let Some(binary) = args.get("cloudflared") {
        Some(CloudflaredConfig::new(binary)) // Explicit trusted external override.
    } else {
        let cache = args
            .get("provider-cache")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(|home| std::path::PathBuf::from(home).join("Library/Caches"))
            })
            .ok_or("--provider-cache required")?;
        if let Some(native) = args.get("native-library-dir") {
            Some(CloudflaredConfig::android_native(native, &cache)?)
        } else {
            Some(CloudflaredConfig::bundled(&cache)?)
        }
    };
    let client = Client::new(cfg)?;
    let store = args.get("credentials").map(CredentialStore::new);
    if let Some(store) = store.as_ref() {
        client.use_credential_store(store.clone()).await?;
        client.set_auth_provider(Arc::new(store.clone())).await;
    }
    if command == "logout" {
        client.close().await?;
        store
            .ok_or("--credentials required")?
            .logout(&client)
            .await?;
        println!(
            "{}",
            serde_json::json!({"event":"logout","refreshGrantRevoked":true})
        );
        return Ok(());
    }
    let name = args
        .get("name")
        .map(String::as_str)
        .unwrap_or("Rust SDK Lab");
    if command == "login" {
        if store.is_none() {
            return Err("--credentials required before authorization".into());
        }
        let auth = client.begin_device_authorization().await?;
        println!(
            "{}",
            serde_json::json!({"event":"authorization","userCode":auth.user_code,"verificationUri":auth.verification_uri,"verificationUriComplete":auth.verification_uri_complete})
        );
        let tokens = client
            .complete_device_authorization(&auth, &CancellationToken::new())
            .await?;
        if store.is_none() {
            return Err("--credentials required before authorization".into());
        }
        let s = client
            .login_with_access_token(&tokens.access_token, name)
            .await?;
        println!("{}", serde_json::json!({"event":"login","session":s}));
        client.close().await?;
        return Ok(());
    }
    let token = if let Some(path) = args.get("access-token-file") {
        let parsed = url::Url::parse(&client.config().base_url)?;
        if parsed.scheme() != "http" {
            return Err("access-token-file is restricted to explicit HTTP loopback harness".into());
        }
        use std::os::unix::fs::PermissionsExt;
        if std::fs::symlink_metadata(path)?.permissions().mode() & 0o077 != 0 {
            return Err("token harness file must be private".into());
        }
        client
            .set_auth_provider(Arc::new(ExplicitLoopbackFile(path.into())))
            .await;
        std::fs::read_to_string(path)?.trim().to_owned()
    } else {
        store
            .as_ref()
            .ok_or("--credentials required")?
            .refresh(&client)
            .await?
            .access_token
    };
    let session = client.login_with_access_token(&token, name).await?;
    println!("{}", serde_json::json!({"event":"login","session":session}));
    let mut events = client.subscribe();
    tokio::spawn(async move {
        while let Ok(e) = events.recv().await {
            eprintln!("{}", serde_json::to_string(&e).unwrap_or_default());
        }
    });
    match command.as_str() {
        "devices" => println!("{}", serde_json::to_string(&client.devices().await?)?),
        "host" => {
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let addr = listener.local_addr()?;
            client
                .expose(Port::tcp("echo", addr.port(), "echo"))
                .await?;
            client.enable_remote().await?;
            println!(
                "{}",
                serde_json::json!({"event":"host-ready","deviceId":session.device_id,"port":addr.port()})
            );
            let c: Arc<Client> = client.clone();
            tokio::spawn(async move {
                loop {
                    let Ok((mut socket, _)) = listener.accept().await else {
                        break;
                    };
                    tokio::spawn(async move {
                        let (mut r, mut w) = socket.split();
                        let _ = tokio::io::copy(&mut r, &mut w).await;
                        let _ = w.shutdown().await;
                    });
                    if c.device_session().await.is_none() {
                        break;
                    }
                }
            });
            tokio::signal::ctrl_c().await?;
        }
        "probe" => {
            let peer = args.get("peer").ok_or("--peer required")?;
            let n: usize = args
                .get("bytes")
                .map(|v| v.parse())
                .transpose()?
                .unwrap_or(65536);
            if n > 16 << 20 {
                return Err("probe size <=16MiB".into());
            }
            let port = args.get("port").map(String::as_str).unwrap_or("echo");
            let start = std::time::Instant::now();
            let mapping = client.map(peer, port, "127.0.0.1:0".parse()?).await?;
            println!(
                "{}",
                serde_json::json!({"event":"mapped","addr":mapping.addr.to_string(),"transport":mapping.transport()})
            );
            let mut socket = TcpStream::connect(mapping.addr).await?;
            let data: Vec<u8> = (0..n).map(|i| (i % 251) as u8).collect();
            let expected = hex::encode(Sha256::digest(&data));
            let (mut reader, mut writer) = socket.split();
            let (write, read) = tokio::join!(
                async {
                    writer.write_all(&data).await?;
                    writer.shutdown().await
                },
                async {
                    let mut b = Vec::new();
                    reader.read_to_end(&mut b).await?;
                    Ok::<_, std::io::Error>(b)
                }
            );
            write?;
            let read = read?;
            if read != data {
                return Err("probe bytes mismatch".into());
            }
            println!(
                "{}",
                serde_json::json!({"event":"verified","bytes":read.len(),"sha256":expected,"seconds":start.elapsed().as_secs_f64(),"transport":mapping.transport()})
            );
            mapping.close().await;
        }
        _ => return Err("unknown command".into()),
    }
    client.close().await?;
    Ok(())
}
