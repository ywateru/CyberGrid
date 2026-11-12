use super::*;
use cybergrid_sdk::Config;
use tokio_util::sync::CancellationToken;
#[tokio::test]
async fn cancelled_caller_retains_dispatch_and_generation_barrier_until_exact_reply() {
    let broker = Arc::new(TokenBroker::default());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    broker
        .attach(7, Arc::new(move |r| tx.send(r).map_err(|_| "CLOSED")))
        .await
        .unwrap();
    broker.bind(7, 11).await.unwrap();
    let client = Client::new(Config {
        app_id: "native".into(),
        ..Config::default()
    })
    .unwrap();
    let cancel = CancellationToken::new();
    let r = AuthTokenRequest {
        issuer: "https://fixture.test/oidc".into(),
        client_id: "native".into(),
        resource: "https://fixture.test/api".into(),
        scopes: vec!["connect".into()],
        min_validity: 60,
        force_refresh: true,
        reason: AuthRequestReason::AuthorizationRejected,
        generation: 1,
        cancel: cancel.clone(),
    };
    let provider = IpcProvider {
        broker: broker.clone(),
        sdk_generation: 11,
    };
    let task = tokio::spawn(async move { provider.access_token_for(&client, &r).await });
    let request = rx.recv().await.unwrap();
    assert!(request.force_refresh);
    cancel.cancel();
    assert!(matches!(task.await.unwrap(), Err(Error::Cancelled)));
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(30), broker.bind(7, 12))
            .await
            .is_err()
    );
    assert_eq!(
        broker
            .reply(TokenReply {
                request_id: request.request_id.clone(),
                owner_generation: 8,
                sdk_generation: 11,
                access_token: Some("access".into()),
                failure: None
            })
            .await,
        Err("STALE_REPLY")
    );
    broker
        .reply(TokenReply {
            request_id: request.request_id,
            owner_generation: 7,
            sdk_generation: 11,
            access_token: Some("access".into()),
            failure: None,
        })
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(1), broker.bind(7, 12))
        .await
        .unwrap()
        .unwrap();
    assert!(rx.try_recv().is_err());
}
#[tokio::test]
async fn exact_ipc_ack_replay_is_receipted_without_redispatch_or_raw_token_cache() {
    let broker = Arc::new(TokenBroker::default());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    broker
        .attach(1, Arc::new(move |r| tx.send(r).map_err(|_| "CLOSED")))
        .await
        .unwrap();
    broker.bind(1, 2).await.unwrap();
    let client = Client::new(Config {
        app_id: "native".into(),
        ..Config::default()
    })
    .unwrap();
    let provider = IpcProvider {
        broker: broker.clone(),
        sdk_generation: 2,
    };
    let r = AuthTokenRequest {
        issuer: "https://issuer.test/oidc".into(),
        client_id: "native".into(),
        resource: "https://resource.test".into(),
        scopes: vec!["connect".into()],
        min_validity: 60,
        force_refresh: false,
        reason: AuthRequestReason::Connect,
        generation: 1,
        cancel: CancellationToken::new(),
    };
    let task = tokio::spawn(async move { provider.access_token_for(&client, &r).await });
    let request = rx.recv().await.unwrap();
    let reply = || TokenReply {
        request_id: request.request_id.clone(),
        owner_generation: 1,
        sdk_generation: 2,
        access_token: Some("synthetic-access".into()),
        failure: None,
    };
    broker.reply(reply()).await.unwrap();
    assert_eq!(task.await.unwrap().unwrap(), "synthetic-access");
    broker.reply(reply()).await.unwrap();
    let mut changed = reply();
    changed.access_token = Some("different".into());
    assert_eq!(broker.reply(changed).await, Err("SEQUENCE_CONFLICT"));
    assert_eq!(broker.inner.lock().await.receipts.len(), 1);
    assert!(rx.try_recv().is_err());
}
