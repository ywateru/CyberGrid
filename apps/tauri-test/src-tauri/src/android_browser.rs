//! Small JNI bridge; OAuth strings never enter public View or ADB responses.
use crate::{browser::Begin, state::LabState};
use jni::{
    objects::{JObject, JString, JValue},
    sys::{jboolean, jstring},
    JNIEnv,
};
use std::sync::{Arc, OnceLock, Weak};
static STATE: OnceLock<Weak<LabState>> = OnceLock::new();
pub fn register(state: &Arc<LabState>) {
    let _ = STATE.set(Arc::downgrade(state));
}
async fn invoke(
    method: &'static str,
    id: &str,
    config: Option<String>,
    cancellation: Option<tokio_util::sync::CancellationToken>,
) -> Result<(), String> {
    let id = id.to_owned();
    let (send, receive) = tokio::sync::oneshot::channel();
    let queued_cancel = cancellation.clone();
    // ndk_context contains the Application, not the Activity. Dispatch through
    // Wry's documented Android context so calls use the current real Activity.
    tauri::wry::prelude::dispatch(move |env, activity, _webview| {
        if queued_cancel.as_ref().is_some_and(|c| c.is_cancelled()) {
            let _ = send.send(Err("Android 登录已取消".to_string()));
            return;
        }
        let result = (|| -> jni::errors::Result<()> {
            let id = JObject::from(env.new_string(id)?);
            if let Some(config) = config {
                let config = JObject::from(env.new_string(config)?);
                env.call_method(
                    activity,
                    method,
                    "(Ljava/lang/String;Ljava/lang/String;)V",
                    &[JValue::Object(&id), JValue::Object(&config)],
                )?;
            } else {
                env.call_method(
                    activity,
                    method,
                    "(Ljava/lang/String;)V",
                    &[JValue::Object(&id)],
                )?;
            }
            Ok(())
        })();
        let result = result.map_err(|_| {
            let _ = env.exception_clear();
            "Android 登录桥不可用".to_string()
        });
        let _ = send.send(result);
    });
    match tokio::time::timeout(std::time::Duration::from_secs(5), receive).await {
        Ok(Ok(result)) => result,
        _ => {
            if let Some(cancel) = cancellation {
                cancel.cancel();
            }
            Err("Android 登录桥不可用".into())
        }
    }
}
pub async fn start(
    begin: &Begin,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<(), String> {
    invoke(
        "startBrowserLogin",
        &begin.transaction_id,
        Some(serde_json::to_string(begin).map_err(|_| "Android 登录桥不可用".to_string())?),
        Some(cancel),
    )
    .await
}
pub async fn cancel(id: &str) -> Result<(), String> {
    invoke("cancelBrowserLogin", id, None, None).await
}
#[no_mangle]
pub extern "system" fn Java_tech_tinadev_cybergrid_lab_MainActivity_browserStage(
    mut env: JNIEnv,
    _this: JObject,
    id: JString,
    payload: JString,
    final_grant: jboolean,
) -> jstring {
    let result = (|| -> Result<String, String> {
        let id: String = env.get_string(&id).map_err(|_| "Rejected")?.into();
        let payload: String = env.get_string(&payload).map_err(|_| "Rejected")?.into();
        if payload.len() > 2 * 1024 * 1024 {
            return Err("Rejected".into());
        }
        let tokens = serde_json::from_str(&payload).map_err(|_| "Rejected")?;
        let state = STATE.get().and_then(Weak::upgrade).ok_or("Rejected")?;
        tauri::async_runtime::block_on(state.android_browser_stage(&id, tokens, final_grant != 0))
    })()
    .unwrap_or_else(|_| "Rejected".into());
    env.new_string(result)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}
#[no_mangle]
pub extern "system" fn Java_tech_tinadev_cybergrid_lab_MainActivity_browserFailed(
    mut env: JNIEnv,
    _this: JObject,
    id: JString,
) {
    if let (Ok(id), Some(state)) = (env.get_string(&id), STATE.get().and_then(Weak::upgrade)) {
        let id: String = id.into();
        tauri::async_runtime::spawn(async move {
            if let Ok(tx) = state.browser_transaction(&id).await {
                tx.source_active
                    .store(false, std::sync::atomic::Ordering::SeqCst);
                tx.source_settled.notify_waiters();
                let _ = state.browser_cancel(&id).await;
            }
        });
    }
}

#[no_mangle]
pub extern "system" fn Java_tech_tinadev_cybergrid_lab_MainActivity_browserAbort(
    mut env: JNIEnv,
    _this: JObject,
    id: JString,
    payload: JString,
) -> jstring {
    let result = (|| -> Result<String, String> {
        let id: String = env.get_string(&id).map_err(|_| "Rejected")?.into();
        let payload: String = env.get_string(&payload).map_err(|_| "Rejected")?.into();
        if payload.len() > 2 * 1024 * 1024 {
            return Err("Rejected".into());
        }
        let tokens = serde_json::from_str(&payload).map_err(|_| "Rejected")?;
        let state = STATE.get().and_then(Weak::upgrade).ok_or("Rejected")?;
        tauri::async_runtime::block_on(async {
            let tx = state.browser_transaction(&id).await?;
            {
                let _lock = tx.operation.lock().await;
                // RAM ownership is installed before a disk failure; cancellation
                // can still revoke the newest grant without a writable volume.
                let _checkpoint = tx.stage(tokens).await;
                tx.cancel.cancel();
                tx.source_active
                    .store(false, std::sync::atomic::Ordering::SeqCst);
                tx.source_settled.notify_waiters();
            }
            tx.cleanup().await?;
            Ok::<String, String>("Cancelled".into())
        })
    })()
    .unwrap_or_else(|_| "Rejected".into());
    env.new_string(result)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}
