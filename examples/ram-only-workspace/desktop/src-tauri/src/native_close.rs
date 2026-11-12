//! The same owned future closes the SDK's synchronous local gate and finishes cleanup.
use cybergrid_sdk::{Client, RemoteDisableOutcome};
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};
use tokio::{runtime::Handle, sync::Notify};
type Close = Pin<Box<dyn Future<Output = RemoteDisableOutcome> + Send + 'static>>;
pub struct NativeClose {
    pending: Mutex<Option<Close>>,
    result: Mutex<Option<RemoteDisableOutcome>>,
    done: Notify,
    runtime: Handle,
}
impl NativeClose {
    pub fn new(client: Arc<Client>, runtime: Handle) -> Arc<Self> {
        Arc::new(Self {
            pending: Mutex::new(Some(Box::pin(async move { client.disable_remote().await }))),
            result: Mutex::new(None),
            done: Notify::new(),
            runtime,
        })
    }
    /// SDK close must keep its local atomics/cancellation before its first await.
    /// No host/control lock is held; the receipt mutex is never used by SDK callbacks.
    pub fn seal(self: &Arc<Self>) {
        let mut slot = self.pending.lock().unwrap();
        let Some(mut future) = slot.take() else {
            return;
        };
        let mut cx = Context::from_waker(futures_util::task::noop_waker_ref());
        let result = future.as_mut().poll(&mut cx);
        match result {
            Poll::Ready(result) => self.finish(result),
            Poll::Pending => {
                let owner = self.clone();
                // The exact future is immediately repolled by Tokio with a real waker.
                self.runtime.spawn(async move {
                    let result = future.await;
                    owner.finish(result);
                });
            }
        }
    }
    fn finish(&self, result: RemoteDisableOutcome) {
        *self.result.lock().unwrap() = Some(result);
        self.done.notify_waiters();
    }
    pub async fn wait(self: &Arc<Self>) -> RemoteDisableOutcome {
        self.seal();
        loop {
            let wake = self.done.notified();
            if let Some(v) = *self.result.lock().unwrap() {
                return v;
            }
            wake.await;
        }
    }
}
