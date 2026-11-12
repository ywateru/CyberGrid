pub mod auth_provider;
pub mod gateway_host;
pub mod host;
pub mod local_browser;
mod native_close;
pub mod oauth_callback;
pub mod oauth_transport;
use crate::{
    auth_provider::{TokenReply, TokenRequest},
    host::{BuildConfig, Host},
    oauth_transport::{FetchRequest, FetchResponse},
};
use std::sync::Arc;
use tauri::{ipc::Channel, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;
fn caller(window: &tauri::WebviewWindow) -> Result<(), &'static str> {
    let url = window.url().map_err(|_| "IPC_FORBIDDEN")?;
    if !local_browser::allowed_caller(window.label(), url.as_str(), cfg!(debug_assertions)) {
        return Err("IPC_FORBIDDEN");
    };
    Ok(())
}
#[tauri::command]
async fn sample_view(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, Arc<Host>>,
) -> Result<host::View, &'static str> {
    caller(&window)?;
    Ok(state.view().await)
}
#[tauri::command]
async fn local_open(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, Arc<Host>>,
    routing: tauri::State<'_, local_browser::LocalBrowserRouting>,
) -> Result<(), &'static str> {
    local_browser::open_local(
        caller(&window),
        || state.local.open_url(),
        routing.inner().0.clone(),
        |url| {
            window
                .app_handle()
                .opener()
                .open_url(url, None::<&str>)
                .map_err(|_| local_browser::LocalBrowserOpenError)
        },
    )
    .await
}
#[tauri::command]
async fn owner_begin(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, Arc<Host>>,
) -> Result<host::Begin, &'static str> {
    caller(&window)?;
    let host = state.inner().clone();
    tokio::spawn(async move { host.begin().await })
        .await
        .map_err(|_| "OWNER_FAILED")?
}
#[tauri::command]
async fn owner_attach(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, Arc<Host>>,
    owner_generation: u64,
    channel: Channel<TokenRequest>,
) -> Result<(), &'static str> {
    caller(&window)?;
    state.oauth(owner_generation).await?;
    state
        .broker
        .attach(
            owner_generation,
            Arc::new(move |request| channel.send(request).map_err(|_| "OWNER_UNAVAILABLE")),
        )
        .await
}
#[tauri::command]
async fn oauth_fetch(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, Arc<Host>>,
    owner_generation: u64,
    operation_id: u64,
    request: FetchRequest,
) -> Result<FetchResponse, &'static str> {
    caller(&window)?;
    state
        .oauth(owner_generation)
        .await?
        .fetch(operation_id, request)
        .await
}
#[tauri::command]
async fn oauth_open(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, Arc<Host>>,
    owner_generation: u64,
    url: String,
) -> Result<(), &'static str> {
    caller(&window)?;
    state.oauth(owner_generation).await?.authorize(&url).await?;
    window
        .app_handle()
        .opener()
        .open_url(url, None::<&str>)
        .map_err(|_| "BROWSER_OPEN_FAILED")
}
#[tauri::command]
async fn oauth_callback(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, Arc<Host>>,
    owner_generation: u64,
) -> Result<String, &'static str> {
    caller(&window)?;
    state.callback(owner_generation).await
}
#[tauri::command]
async fn sdk_prepare(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, Arc<Host>>,
    owner_generation: u64,
) -> Result<u64, &'static str> {
    caller(&window)?;
    state.prepare_sdk(owner_generation).await
}
#[tauri::command]
async fn sdk_login(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, Arc<Host>>,
    owner_generation: u64,
    sdk_generation: u64,
) -> Result<(), &'static str> {
    caller(&window)?;
    let host = state.inner().clone();
    tokio::spawn(async move { host.login(owner_generation, sdk_generation).await })
        .await
        .map_err(|_| "SDK_FAILED")?
}
#[tauri::command]
async fn token_reply(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, Arc<Host>>,
    reply: TokenReply,
) -> Result<(), &'static str> {
    caller(&window)?;
    state.broker.reply(reply).await
}
#[tauri::command]
async fn remote_enable(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, Arc<Host>>,
) -> Result<(), &'static str> {
    caller(&window)?;
    let host = state.inner().clone();
    tokio::spawn(async move { host.enable().await })
        .await
        .map_err(|_| "REMOTE_FAILED")?
}
#[tauri::command]
async fn remote_disable(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, Arc<Host>>,
) -> Result<(), &'static str> {
    caller(&window)?;
    state.seal();
    let host = state.inner().clone();
    tokio::spawn(async move { host.disable().await })
        .await
        .map_err(|_| "CLEANUP_PENDING")?
}
#[tauri::command]
async fn owner_end(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, Arc<Host>>,
    owner_generation: u64,
) -> Result<(), &'static str> {
    caller(&window)?;
    state.seal();
    let host = state.inner().clone();
    tokio::spawn(async move { host.end_owner(owner_generation).await })
        .await
        .map_err(|_| "CLEANUP_PENDING")?
}
#[tauri::command]
async fn finish_exit(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, Arc<Host>>,
) -> Result<(), &'static str> {
    caller(&window)?;
    state.allow_exit().await?;
    window.app_handle().exit(0);
    Ok(())
}
pub fn run() {
    run_with_factory(BuildConfig::default(), None)
}
/// Reuses the exact public IPC, navigation and exit ownership with a trusted factory.
pub fn run_with_factory(
    config: BuildConfig,
    factory: Option<Arc<dyn gateway_host::GatewayFactory>>,
) {
    run_assembled(config, factory, None)
}
/// Trusted process assembly may route only the initial local-browser open.
/// OAuth retains its original system-browser path.
pub fn run_with_factory_and_local_opener(
    config: BuildConfig,
    factory: Option<Arc<dyn gateway_host::GatewayFactory>>,
    opener: Arc<dyn local_browser::LocalBrowserOpener>,
) {
    run_assembled(config, factory, Some(opener))
}
fn run_assembled(
    config: BuildConfig,
    factory: Option<Arc<dyn gateway_host::GatewayFactory>>,
    opener: Option<Arc<dyn local_browser::LocalBrowserOpener>>,
) {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(local_browser::LocalBrowserRouting(opener))
        .setup(move |app| {
            let host = tauri::async_runtime::block_on(Host::new(config, factory))
                .map_err(std::io::Error::other)?;
            app.manage(host);
            tauri::WebviewWindowBuilder::from_config(app, &app.config().app.windows[0])?
                .on_navigation(|url| {
                    url.as_str() == "http://127.0.0.1:1430/" || url.as_str() == "tauri://localhost/"
                })
                .build()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            sample_view,
            local_open,
            owner_begin,
            owner_attach,
            oauth_fetch,
            oauth_open,
            oauth_callback,
            sdk_prepare,
            sdk_login,
            token_reply,
            remote_enable,
            remote_disable,
            owner_end,
            finish_exit
        ])
        .build(tauri::generate_context!())
        .expect("workspace startup failed");
    app.run(|app, event| {
        let host = app.state::<Arc<Host>>();
        if host.exit_allowed.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        };
        match event {
            tauri::RunEvent::ExitRequested { api, .. } => {
                api.prevent_exit();
                host.seal();
                let _ = app.emit_to("main", "sample-shutdown", ());
            }
            tauri::RunEvent::WindowEvent {
                event: tauri::WindowEvent::CloseRequested { api, .. },
                ..
            } => {
                api.prevent_close();
                host.seal();
                let _ = app.emit_to("main", "sample-shutdown", ());
            }
            _ => {}
        }
    });
}
