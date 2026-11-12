//! Trusted initial routing of a freshly minted local-browser capability.
//! This extension does not participate in OAuth or expose a URL through IPC.
use std::{future::Future, pin::Pin, sync::Arc};

/// Fixed failure marker. Implementations must not log or persist the supplied URL.
#[derive(Clone, Copy, Debug)]
pub struct LocalBrowserOpenError;

pub type LocalBrowserOpenFuture =
    Pin<Box<dyn Future<Output = Result<(), LocalBrowserOpenError>> + Send + 'static>>;

/// Installed only by trusted native process assembly, never by web input.
pub trait LocalBrowserOpener: Send + Sync {
    fn open(&self, url: String) -> LocalBrowserOpenFuture;
}

pub(crate) struct LocalBrowserRouting(pub Option<Arc<dyn LocalBrowserOpener>>);

pub(crate) fn allowed_caller(label: &str, url: &str, debug: bool) -> bool {
    label == "main" && (url == "tauri://localhost/" || (debug && url == "http://127.0.0.1:1430/"))
}

pub(crate) async fn open_local<M, F, S>(
    authorized: Result<(), &'static str>,
    mint: M,
    custom: Option<Arc<dyn LocalBrowserOpener>>,
    system: S,
) -> Result<(), &'static str>
where
    M: FnOnce() -> F,
    F: Future<Output = Result<String, &'static str>>,
    S: FnOnce(String) -> Result<(), LocalBrowserOpenError>,
{
    authorized?;
    let url = mint().await?;
    if let Some(opener) = custom {
        // Once dispatched, the initial open remains owned if its IPC waiter is
        // cancelled. Neither the capability nor an implementation error is logged.
        tokio::spawn(async move { opener.open(url).await })
            .await
            .map_err(|_| "BROWSER_OPEN_FAILED")?
            .map_err(|_| "BROWSER_OPEN_FAILED")
    } else {
        system(url).map_err(|_| "BROWSER_OPEN_FAILED")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Held {
        entered: tokio::sync::mpsc::UnboundedSender<()>,
        release: Arc<tokio::sync::Notify>,
        completed: Arc<AtomicUsize>,
        fail: bool,
    }
    impl LocalBrowserOpener for Held {
        fn open(&self, url: String) -> LocalBrowserOpenFuture {
            let entered = self.entered.clone();
            let release = self.release.clone();
            let completed = self.completed.clone();
            let fail = self.fail;
            Box::pin(async move {
                assert!(url.starts_with("http://127.0.0.1:"));
                entered.send(()).unwrap();
                release.notified().await;
                completed.fetch_add(1, Ordering::SeqCst);
                if fail {
                    Err(LocalBrowserOpenError)
                } else {
                    Ok(())
                }
            })
        }
    }

    #[tokio::test]
    async fn default_path_mints_once_and_uses_system_opener() {
        let minted = AtomicUsize::new(0);
        let opened = AtomicUsize::new(0);
        open_local(
            Ok(()),
            || async {
                minted.fetch_add(1, Ordering::SeqCst);
                Ok("synthetic".into())
            },
            None,
            |url| {
                assert_eq!(url, "synthetic");
                opened.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        )
        .await
        .unwrap();
        assert_eq!(minted.load(Ordering::SeqCst), 1);
        assert_eq!(opened.load(Ordering::SeqCst), 1);
        assert_eq!(
            open_local(
                Ok(()),
                || async { Ok("synthetic".into()) },
                None,
                |_| Err(LocalBrowserOpenError)
            )
            .await,
            Err("BROWSER_OPEN_FAILED")
        );
    }

    #[tokio::test]
    async fn rejected_caller_cannot_mint_or_dispatch() {
        assert_eq!(
            open_local(
                Err("IPC_FORBIDDEN"),
                || async { panic!("must not mint") },
                None,
                |_| panic!("must not open")
            )
            .await,
            Err("IPC_FORBIDDEN")
        );
        assert_eq!(
            open_local(
                Ok(()),
                || async { Err("LOCAL_CAPACITY") },
                None,
                |_| panic!("must not open")
            )
            .await,
            Err("LOCAL_CAPACITY")
        );
    }

    #[test]
    fn exact_main_caller_policy_keeps_release_and_debug_boundaries() {
        assert!(allowed_caller("main", "tauri://localhost/", false));
        assert!(allowed_caller("main", "http://127.0.0.1:1430/", true));
        assert!(!allowed_caller("main", "http://127.0.0.1:1430/", false));
        for url in [
            "tauri://localhost/path",
            "tauri://localhost/#x",
            "https://localhost/",
            "http://127.0.0.1:1430/?x",
        ] {
            assert!(!allowed_caller("main", url, true));
        }
        assert!(!allowed_caller("other", "tauri://localhost/", true));
    }

    #[tokio::test]
    async fn custom_open_is_owned_after_waiter_cancel_and_has_fixed_failure() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let release = Arc::new(tokio::sync::Notify::new());
        let completed = Arc::new(AtomicUsize::new(0));
        let opener = Arc::new(Held {
            entered: tx,
            release: release.clone(),
            completed: completed.clone(),
            fail: true,
        });
        let mut work = Box::pin(open_local(
            Ok(()),
            || async { Ok("http://127.0.0.1:1/#synthetic".into()) },
            Some(opener.clone()),
            |_| panic!("custom must not fall back"),
        ));
        assert!(futures_util::poll!(work.as_mut()).is_pending());
        rx.recv().await.unwrap();
        drop(work);
        release.notify_one();
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while completed.load(Ordering::SeqCst) != 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let second = tokio::spawn(open_local(
            Ok(()),
            || async { Ok("http://127.0.0.1:1/#synthetic".into()) },
            Some(opener),
            |_| panic!("custom must not fall back"),
        ));
        rx.recv().await.unwrap();
        release.notify_one();
        assert_eq!(second.await.unwrap(), Err("BROWSER_OPEN_FAILED"));
    }
}
