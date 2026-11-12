//! Deliberately private debug-test control. Never compiled into a normal app.
use crate::state::{LabConfig, LabState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

const MAX_REQUEST: usize = 8192;
const MAX_RESULT: usize = 1024 * 1024;
const MAX_SEQ: u64 = 9_007_199_254_740_991;
const CACHE: usize = 16;
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    version: u8,
    control_epoch: String,
    sequence: u64,
    request_id: String,
    action: String,
    payload: Value,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Login {
    #[serde(default)]
    server: Option<String>,
    #[serde(default)]
    app: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    force_fallback: bool,
    #[serde(default)]
    direct_strategy: cybergrid_sdk::DirectStrategy,
    #[serde(default)]
    direct_transport: crate::state::DirectTransport,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Device {
    device_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Text {
    text: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
enum Action {
    Login(LabConfig, bool),
    WebLogin(LabConfig),
    Cancel,
    Refresh,
    EnableRemote,
    DisableRemote,
    Connect(String),
    Disconnect,
    Send(String),
    Probe(String),
    Logout,
    Close,
}
fn uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit() && !b.is_ascii_uppercase()
            }
        })
}
fn new_epoch() -> String {
    hex::encode(rand::random::<[u8; 16]>())
}
fn parse(bytes: &[u8]) -> Result<(Request, Action), &'static str> {
    if bytes.len() > MAX_REQUEST {
        return Err("REQUEST_TOO_LARGE");
    }
    let r: Request = serde_json::from_slice(bytes).map_err(|_| "INVALID_REQUEST")?;
    if r.version != 1
        || !uuid(&r.request_id)
        || r.control_epoch.len() != 32
        || !r
            .control_epoch
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        || r.sequence == 0
        || r.sequence > MAX_SEQ
    {
        return Err("INVALID_REQUEST");
    }
    let decode = || -> Result<Action, ()> {
        Ok(match r.action.as_str() {
            "login" | "web-login" | "resume" => {
                let p: Login = serde_json::from_value(r.payload.clone()).map_err(|_| ())?;
                let default = LabConfig::default();
                let server = p.server.unwrap_or(default.base_url);
                let app = p.app.unwrap_or(default.app_id);
                let name = p.name.unwrap_or(default.device_name);
                if server.len() > 2048
                    || !(server.starts_with("https://") || server.starts_with("http://"))
                    || app.is_empty()
                    || app.len() > 256
                    || name.is_empty()
                    || name.len() > 256
                {
                    return Err(());
                }
                let config = LabConfig {
                    base_url: server,
                    app_id: app,
                    device_name: name,
                    force_fallback: p.force_fallback,
                    direct_strategy: p.direct_strategy,
                    direct_transport: p.direct_transport,
                    cloudflared_path: String::new(),
                };
                if r.action != "resume" {
                    Action::WebLogin(config)
                } else {
                    Action::Login(config, r.action == "resume")
                }
            }
            "connect" | "probe" => {
                let p: Device = serde_json::from_value(r.payload.clone()).map_err(|_| ())?;
                if p.device_id.is_empty() || p.device_id.len() > 256 {
                    return Err(());
                }
                if r.action == "connect" {
                    Action::Connect(p.device_id)
                } else {
                    Action::Probe(p.device_id)
                }
            }
            "send" => {
                let p: Text = serde_json::from_value(r.payload.clone()).map_err(|_| ())?;
                if p.text.len() > 4096 {
                    return Err(());
                }
                Action::Send(p.text)
            }
            action => {
                let _: Empty = serde_json::from_value(r.payload.clone()).map_err(|_| ())?;
                match action {
                    "cancel-login" => Action::Cancel,
                    "refresh" => Action::Refresh,
                    "enable-remote" => Action::EnableRemote,
                    "disable-remote" => Action::DisableRemote,
                    "disconnect" => Action::Disconnect,
                    "logout" => Action::Logout,
                    "close" => Action::Close,
                    _ => return Err(()),
                }
            }
        })
    };
    let action = decode().map_err(|_| "INVALID_ACTION_OR_PAYLOAD")?;
    Ok((r, action))
}
fn atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let tmp = path.with_extension("tmp");
    match fs::remove_file(&tmp) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e),
    }
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&tmp)?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(tmp, path)?;
    fs::File::open(path.parent().unwrap())?.sync_all()
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Claim {
    sequence: u64,
    request_id: String,
    digest: String,
}
struct Ledger {
    root: PathBuf,
    epoch: String,
    highwater: u64,
    results: VecDeque<(Claim, Value)>,
    pending: Vec<Claim>,
    completed: VecDeque<Value>,
    published_sequence: u64,
}
enum Acceptance {
    New(Claim),
    Existing(Value),
    InFlight,
}
impl Ledger {
    fn open(root: PathBuf) -> std::io::Result<Self> {
        use std::os::unix::fs::PermissionsExt;
        fs::create_dir_all(&root)?;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
        // Previous epoch files are never scanned or dispatched. Fixed files only.
        for name in [
            "request.json",
            "request.tmp",
            "result.json",
            "result.tmp",
            "claim.json",
            "claim.tmp",
        ] {
            match fs::remove_file(root.join(name)) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(e),
            }
        }
        match fs::remove_dir(root.join("submit-lock")) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e),
        }
        Ok(Self {
            root,
            epoch: new_epoch(),
            highwater: 0,
            results: VecDeque::new(),
            pending: Vec::new(),
            completed: VecDeque::new(),
            published_sequence: 0,
        })
    }
    fn accept(&mut self, r: &Request, bytes: &[u8]) -> Result<Acceptance, &'static str> {
        if r.control_epoch != self.epoch {
            return Err("STALE_EPOCH");
        }
        let claim = Claim {
            sequence: r.sequence,
            request_id: r.request_id.clone(),
            digest: hex::encode(Sha256::digest(bytes)),
        };
        if r.sequence <= self.highwater {
            return match self.results.iter().find(|(c, _)| c.sequence == r.sequence) {
                Some((old, result))
                    if old.request_id == claim.request_id && old.digest == claim.digest =>
                {
                    Ok(Acceptance::Existing(result.clone()))
                }
                Some(_) => Err("SEQUENCE_CONFLICT"),
                None => match self.pending.iter().find(|c| c.sequence == r.sequence) {
                    Some(old)
                        if old.request_id == claim.request_id && old.digest == claim.digest =>
                    {
                        Ok(Acceptance::InFlight)
                    }
                    Some(_) => Err("SEQUENCE_CONFLICT"),
                    None => Err("OUTCOME_UNKNOWN"),
                },
            };
        }
        if r.sequence != self.highwater + 1 {
            return Err("SEQUENCE_GAP");
        }
        let body = json!({"controlEpoch":self.epoch,"highwater":r.sequence,"claim":claim});
        atomic(
            &self.root.join("claim.json"),
            &serde_json::to_vec(&body).unwrap(),
        )
        .map_err(|_| "CLAIM_IO_ERROR")?;
        // Advance only after durable publication and before any operation.
        self.highwater = r.sequence;
        self.pending.push(claim.clone());
        Ok(Acceptance::New(claim))
    }
    fn finish(&mut self, claim: Claim, result: Value) -> std::io::Result<()> {
        self.publish_response(&result)?;
        self.pending.retain(|p| p.sequence != claim.sequence);
        self.results.push_back((claim, result));
        while self.results.len() > CACHE {
            self.results.pop_front();
        }
        Ok(())
    }
    fn publish_response(&mut self, result: &Value) -> std::io::Result<()> {
        let sequence = result["sequence"].as_u64().unwrap_or(0);
        if sequence >= self.published_sequence {
            atomic(&self.root.join("result.json"), &bounded(result)?)?;
            self.published_sequence = sequence;
        }
        self.record_response(result);
        Ok(())
    }
    fn record_response(&mut self, result: &Value) {
        // Outcomes are independently addressable even when a lower sequence
        // finishes later. Only the live status carries state, once, below.
        let summary = select(
            result,
            &[
                "version",
                "controlEpoch",
                "sequence",
                "requestId",
                "ok",
                "code",
            ],
        );
        self.completed.retain(|old| {
            ["controlEpoch", "sequence", "requestId"]
                .iter()
                .any(|key| old[key] != summary[key])
        });
        self.completed.push_back(summary);
        while self.completed.len() > CACHE {
            self.completed.pop_front();
        }
    }
}
fn bounded(value: &Value) -> std::io::Result<Vec<u8>> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > MAX_RESULT {
        return Err(std::io::Error::other("CONTROL_RESULT_TOO_LARGE"));
    }
    Ok(bytes)
}
fn select(value: &Value, fields: &[&str]) -> Value {
    if value.is_null() {
        return Value::Null;
    }
    let mut result = serde_json::Map::new();
    for field in fields {
        if let Some(v) = value.get(*field) {
            result.insert((*field).to_string(), v.clone());
        }
    }
    Value::Object(result)
}
fn public_projection(source: &Value) -> Value {
    let mut value = select(
        source,
        &[
            "phase",
            "canResume",
            "platform",
            "authMode",
            "remoteAction",
            "localClosed",
            "coordinatorRevocation",
            "directoryKnown",
        ],
    );
    value["sdkSnapshot"] = select(
        &source["sdkSnapshot"],
        &[
            "revision",
            "generation",
            "epoch",
            "identity",
            "coordination",
            "publication",
            "reachability",
            "businessAuthorization",
            "remoteEnabled",
            "intentRevision",
            "confirmedIntentRevision",
            "serverPublishRevision",
        ],
    );
    if value["sdkSnapshot"].is_object() {
        for layer in ["identity", "coordination", "publication", "reachability"] {
            value["sdkSnapshot"][layer] = select(
                &source["sdkSnapshot"][layer],
                &["phase", "reason", "retryable"],
            );
        }
    }
    value["config"] = select(
        &source["config"],
        &[
            "baseUrl",
            "appId",
            "deviceName",
            "forceFallback",
            "directStrategy",
            "directTransport",
        ],
    );
    value["local"] = select(
        &source["local"],
        &["deviceId", "deviceName", "userId", "ports"],
    );
    value["connection"] = select(
        &source["connection"],
        &[
            "peerId",
            "peerName",
            "localAddress",
            "transport",
            "connected",
        ],
    );
    for (name, fields) in [
        (
            "devices",
            &["id", "name", "online", "lastSeen", "ports"][..],
        ),
        (
            "records",
            &[
                "id",
                "time",
                "direction",
                "peer",
                "kind",
                "message",
                "bytes",
                "sha256",
                "transport",
            ][..],
        ),
    ] {
        value[name] = Value::Array(
            source[name]
                .as_array()
                .map(|rows| rows.iter().map(|row| select(row, fields)).collect())
                .unwrap_or_default(),
        );
    }
    // Port DTO also has an explicit whitelist.
    if let Some(ports) = value
        .get_mut("local")
        .and_then(|v| v.get_mut("ports"))
        .and_then(Value::as_array_mut)
    {
        for port in ports {
            *port = select(port, &["id", "port", "type", "transport"]);
        }
    }
    if let Some(devices) = value["devices"].as_array_mut() {
        for device in devices {
            if let Some(ports) = device.get_mut("ports").and_then(Value::as_array_mut) {
                for port in ports {
                    *port = select(port, &["id", "port", "type", "transport"]);
                }
            }
        }
    }
    value
}
async fn safe_view(state: &LabState) -> Value {
    let mut value =
        serde_json::to_value(state.view().await).unwrap_or_else(|_| json!({"phase":"unavailable"}));
    // Explicit output whitelist: adding a future View field cannot export it here.
    value = public_projection(&value);
    let mut truncated = false;
    while serde_json::to_vec(&value)
        .map(|v| v.len() > MAX_RESULT / 2)
        .unwrap_or(true)
    {
        if let Some(records) = value.get_mut("records").and_then(Value::as_array_mut) {
            if !records.is_empty() {
                records.remove(0);
                truncated = true;
                continue;
            }
        }
        value = json!({"phase":"snapshot_too_large"});
        truncated = true;
        break;
    }
    value["recordsTruncated"] = json!(truncated);
    value
}
#[cfg(test)]
pub(crate) async fn dispatch_public_test(
    state: &Arc<LabState>,
    bytes: &[u8],
) -> Result<Value, String> {
    let (_, action) = parse(bytes).map_err(str::to_owned)?;
    dispatch(state, action).await?;
    Ok(safe_view(state).await)
}
async fn dispatch(state: &Arc<LabState>, action: Action) -> Result<(), String> {
    match action {
        Action::Login(config, resume) => state.start_login(config, resume).await,
        Action::WebLogin(config) => state
            .begin_web_login(config)
            .await
            .map(|_| ())
            .map_err(|error| error.message().to_string()),
        Action::Cancel => state.cancel_login().await,
        Action::Refresh => state.refresh_devices().await,
        Action::EnableRemote => state.start_enable_remote().await,
        Action::DisableRemote => state.disable_remote().await.map(|_| ()),
        Action::Connect(id) => state.connect_device(id).await,
        Action::Disconnect => state.disconnect_device().await,
        Action::Send(text) => state.send_message(text).await,
        Action::Probe(id) => state.probe_device(id).await,
        Action::Logout => state.shutdown(true).await,
        Action::Close => state.shutdown(false).await,
    }
}
fn read_request(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take((MAX_REQUEST + 1) as u64)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}
#[cfg(all(target_os = "android", debug_assertions, feature = "adb-test-control"))]
pub fn start(root: PathBuf, state: Arc<LabState>, handle: tauri::AppHandle) -> std::io::Result<()> {
    let ledger = Ledger::open(root)?;
    tauri::async_runtime::spawn(run(ledger, state, move || handle.exit(0)));
    Ok(())
}
async fn publish_status(ledger: &Ledger, state: &LabState) -> std::io::Result<()> {
    let status = json!({"version":1,"controlEpoch":ledger.epoch,"pid":std::process::id(),
        "highwater":ledger.highwater,"completed":ledger.completed,"state":safe_view(state).await});
    atomic(&ledger.root.join("status.json"), &bounded(&status)?)
}
struct Completion {
    claim: Claim,
    priority: bool,
    close: bool,
    ok: bool,
}
async fn run(mut ledger: Ledger, state: Arc<LabState>, exit: impl Fn() + Send + 'static) {
    let mut work = tokio::task::JoinSet::new();
    let mut priority_active = false;
    let mut stopping = false;
    let mut exit_requested = false;
    let mut tick = tokio::time::interval(std::time::Duration::from_millis(200));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        if stopping && work.is_empty() {
            if exit_requested {
                state
                    .exiting
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                exit();
            }
            return;
        }
        tokio::select! {
            completion = work.join_next(), if !work.is_empty() => {
                match completion {
                    Some(Ok(Completion {claim,priority,close,ok})) => {
                        if priority {priority_active=false;}
                        let value=json!({"version":1,"controlEpoch":ledger.epoch,"requestId":claim.request_id,
                            "sequence":claim.sequence,"ok":ok,"code":if ok {"OK"} else {"APP_ERROR"},"state":safe_view(&state).await});
                        if ledger.finish(claim,value).is_err() { stopping=true; }
                        if close && ok { exit_requested=true;stopping=true; }
                        if publish_status(&ledger,&state).await.is_err() {stopping=true;}
                    }
                    // A panicked operation has an unknown result. Never dispatch
                    // its claim again; drain all other owned work before return.
                    _ => stopping=true,
                }
            }
            _=tick.tick()=> {
                if publish_status(&ledger,&state).await.is_err() {stopping=true;}
                if stopping {continue;}
                let path=ledger.root.join("request.json");
                let bytes=match read_request(&path) {
                    Ok(bytes)=>bytes,
                    Err(error) if error.kind()==std::io::ErrorKind::NotFound=>continue,
                    Err(_)=>{stopping=true;continue;}
                };
                match parse(&bytes) {
                    Ok((request,action))=>match ledger.accept(&request,&bytes) {
                        Ok(Acceptance::New(claim))=> {
                            // Claim is durable; publish its highwater before
                            // freeing the mailbox or dispatching any effect.
                            if publish_status(&ledger,&state).await.is_err() {
                                stopping=true;continue;
                            }
                            if fs::remove_file(&path).is_err() {stopping=true;continue;}
                            let priority=matches!(action,Action::DisableRemote);
                            if !work.is_empty() && (!priority || priority_active) {
                                let value=json!({"version":1,"controlEpoch":ledger.epoch,"requestId":claim.request_id,
                                    "sequence":claim.sequence,"ok":false,"code":"BUSY","state":safe_view(&state).await});
                                if ledger.finish(claim,value).is_err() {stopping=true;}
                            } else {
                                let close=matches!(action,Action::Close);
                                priority_active|=priority;
                                let state=state.clone();
                                work.spawn(async move { let ok=dispatch(&state,action).await.is_ok();Completion{claim,priority,close,ok} });
                            }
                        }
                        Ok(Acceptance::InFlight)=> {
                            // Proven same owned operation: join its eventual
                            // outcome by identity, without a waiter task, false
                            // terminal result, or occupying the mailbox.
                            if fs::remove_file(&path).is_err() {stopping=true;}
                        }
                        Ok(Acceptance::Existing(value))=> {
                            if ledger.publish_response(&value).is_err() {stopping=true;}
                            if fs::remove_file(&path).is_err() {stopping=true;}
                        }
                        Err(code)=> {
                            let value=json!({"version":1,"controlEpoch":ledger.epoch,"requestId":request.request_id,
                                "sequence":request.sequence,"ok":false,"code":code});
                            ledger.record_response(&value);
                            if fs::remove_file(&path).is_err() {stopping=true;}
                        }
                    },
                    Err(code)=> {
                        let untrusted:Value=serde_json::from_slice(&bytes).unwrap_or(Value::Null);
                        let id=untrusted.get("requestId").and_then(Value::as_str).filter(|v|uuid(v)).unwrap_or("");
                        let sequence=untrusted.get("sequence").and_then(Value::as_u64).unwrap_or(0);
                        let value=json!({"version":1,"controlEpoch":ledger.epoch,"requestId":id,"sequence":sequence,"ok":false,"code":code});
                        ledger.record_response(&value);
                        if fs::remove_file(&path).is_err() {stopping=true;}
                    }
                }
                if publish_status(&ledger,&state).await.is_err() {stopping=true;}
            }
        }
        // No early return while a dispatched authorization/cleanup future is
        // owned. IO failure stops admission, not checkpoint completion.
    }
}
#[cfg(test)]
pub(crate) async fn run_test(root: PathBuf, state: Arc<LabState>) {
    run(Ledger::open(root).unwrap(), state, || {}).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root() -> PathBuf {
        std::env::temp_dir().join(format!("cybergrid-adb-{}", new_epoch()))
    }
    fn request(ledger: &Ledger, seq: u64) -> Vec<u8> {
        serde_json::to_vec(&json!({"version":1,"controlEpoch":ledger.epoch,"sequence":seq,"requestId":format!("00000000-0000-4000-8000-{seq:012x}"),"action":"refresh","payload":{}})).unwrap()
    }
    #[test]
    fn transport_preference_is_bounded_and_shared_by_browser_login_and_resume() {
        let directory = root();
        let ledger = Ledger::open(directory.clone()).unwrap();
        let mut value: Value = serde_json::from_slice(&request(&ledger, 1)).unwrap();
        for action in ["web-login", "resume"] {
            value["action"] = json!(action);
            for (mode, expected) in [
                ("auto", crate::state::DirectTransport::Auto),
                ("webrtc-only", crate::state::DirectTransport::WebrtcOnly),
                (
                    "iroh-candidate",
                    crate::state::DirectTransport::IrohCandidate,
                ),
            ] {
                value["payload"] = json!({"directTransport": mode});
                let (_, parsed) = parse(&serde_json::to_vec(&value).unwrap()).unwrap();
                let config = match parsed {
                    Action::WebLogin(c) | Action::Login(c, true) => c,
                    _ => panic!(),
                };
                assert_eq!(config.direct_transport, expected);
                assert!(!config.force_fallback);
                assert!(config.cloudflared_path.is_empty());
            }
            value["payload"] = json!({"directTransport": "iroh-only"});
            assert!(parse(&serde_json::to_vec(&value).unwrap()).is_err());
        }
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn strict_inputs_and_size() {
        let directory = root();
        let ledger = Ledger::open(directory.clone()).unwrap();
        let bytes = request(&ledger, 1);
        assert!(parse(&bytes).is_ok());
        let base: Value = serde_json::from_slice(&bytes).unwrap();
        for patch in [
            json!({"extra":true}),
            json!({"sequence":0}),
            json!({"sequence":MAX_SEQ+1}),
            json!({"requestId":"../../escape"}),
            json!({"action":"exec"}),
            json!({"payload":{"token":"secret"}}),
            json!({"version":2}),
            json!({"payload":null}),
        ] {
            let mut value = base.clone();
            for (k, v) in patch.as_object().unwrap() {
                value[k] = v.clone();
            }
            assert!(
                parse(&serde_json::to_vec(&value).unwrap()).is_err(),
                "{value}"
            );
        }
        let mut login = base.clone();
        login["action"] = json!("login");
        login["payload"] = json!({"server":"https://example.test","app":"id","name":"Android","cloudflaredPath":"/bin/sh"});
        assert!(parse(&serde_json::to_vec(&login).unwrap()).is_err());
        login["payload"]
            .as_object_mut()
            .unwrap()
            .remove("cloudflaredPath");
        let (_, Action::WebLogin(config)) = parse(&serde_json::to_vec(&login).unwrap()).unwrap()
        else {
            panic!()
        };
        assert!(config.cloudflared_path.is_empty());
        assert!(parse(&vec![b' '; MAX_REQUEST + 1]).is_err());
        assert!(bounded(&json!({"data":"x".repeat(MAX_RESULT)})).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn remote_control_strict_empty_payload_and_duplicate_ledger() {
        for action in ["enable-remote", "disable-remote"] {
            let directory = root();
            let mut ledger = Ledger::open(directory.clone()).unwrap();
            let mut value: Value = serde_json::from_slice(&request(&ledger, 1)).unwrap();
            value["action"] = json!(action);
            let bytes = serde_json::to_vec(&value).unwrap();
            let (request, parsed) = parse(&bytes).unwrap();
            assert!(matches!(
                parsed,
                Action::EnableRemote | Action::DisableRemote
            ));
            let Acceptance::New(claim) = ledger.accept(&request, &bytes).unwrap() else {
                panic!()
            };
            let result = json!({"localClosed":true,"coordinatorRevocation":"pending"});
            ledger.finish(claim, result.clone()).unwrap();
            assert!(
                matches!(ledger.accept(&request,&bytes),Ok(Acceptance::Existing(existing)) if existing==result)
            );
            for payload in [
                json!({"token":"secret"}),
                json!({"enabled":true}),
                json!(null),
            ] {
                value["payload"] = payload;
                assert!(parse(&serde_json::to_vec(&value).unwrap()).is_err());
            }
            fs::remove_dir_all(directory).unwrap();
        }
    }
    #[test]
    fn durable_claim_cache_fence_and_new_boot() {
        let directory = root();
        let mut ledger = Ledger::open(directory.clone()).unwrap();
        let bytes = request(&ledger, 1);
        let (r, _) = parse(&bytes).unwrap();
        let Acceptance::New(claim) = ledger.accept(&r, &bytes).unwrap() else {
            panic!()
        };
        let disk: Value =
            serde_json::from_slice(&fs::read(directory.join("claim.json")).unwrap()).unwrap();
        assert_eq!(disk["highwater"], 1);
        // Claimed but unfinished never executes again, even before a restart.
        assert!(matches!(
            ledger.accept(&r, &bytes),
            Ok(Acceptance::InFlight)
        ));
        let changed = bytes.iter().copied().chain(*b" ").collect::<Vec<_>>();
        assert!(matches!(
            ledger.accept(&r, &changed),
            Err("SEQUENCE_CONFLICT")
        ));
        let result = json!({"ok":true,"sequence":1});
        ledger.finish(claim, result.clone()).unwrap();
        assert!(
            matches!(ledger.accept(&r,&bytes),Ok(Acceptance::Existing(value)) if value==result)
        );
        let changed = bytes.iter().copied().chain(*b" ").collect::<Vec<_>>();
        assert!(matches!(
            ledger.accept(&r, &changed),
            Err("SEQUENCE_CONFLICT")
        ));
        let future = request(&ledger, 3);
        let (future_r, _) = parse(&future).unwrap();
        assert!(matches!(
            ledger.accept(&future_r, &future),
            Err("SEQUENCE_GAP")
        ));
        for seq in 2..=18 {
            let b = request(&ledger, seq);
            let (r, _) = parse(&b).unwrap();
            let Acceptance::New(c) = ledger.accept(&r, &b).unwrap() else {
                panic!()
            };
            ledger.finish(c, json!({"sequence":seq})).unwrap();
        }
        assert_eq!(ledger.results.len(), 16);
        assert!(matches!(ledger.accept(&r, &bytes), Err("OUTCOME_UNKNOWN")));
        atomic(&directory.join("request.json"), &bytes).unwrap();
        let mut next = Ledger::open(directory.clone()).unwrap();
        assert_ne!(next.epoch, ledger.epoch);
        assert!(!directory.join("request.json").exists());
        assert!(matches!(next.accept(&r, &bytes), Err("STALE_EPOCH")));
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn failed_claim_cannot_advance_or_execute() {
        let directory = root();
        let mut ledger = Ledger::open(directory.clone()).unwrap();
        fs::create_dir(directory.join("claim.tmp")).unwrap();
        let bytes = request(&ledger, 1);
        let (r, _) = parse(&bytes).unwrap();
        assert!(matches!(ledger.accept(&r, &bytes), Err("CLAIM_IO_ERROR")));
        assert_eq!(ledger.highwater, 0);
        assert!(ledger.results.is_empty());
        assert!(!directory.join("claim.json").exists());
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn late_completion_keeps_latest_result_and_bounded_metadata_only_cache() {
        let directory = root();
        let mut ledger = Ledger::open(directory.clone()).unwrap();
        let mut claims = Vec::new();
        for sequence in 1..=2 {
            let bytes = request(&ledger, sequence);
            let (r, _) = parse(&bytes).unwrap();
            let Acceptance::New(claim) = ledger.accept(&r, &bytes).unwrap() else {
                panic!()
            };
            claims.push(claim);
        }
        let old = claims.remove(0);
        let new = claims.remove(0);
        let outcome = |claim: &Claim| json!({"version":1,"controlEpoch":ledger.epoch,"sequence":claim.sequence,"requestId":claim.request_id,"ok":true,"code":"OK","state":{"localClosed":claim.sequence==2,"private":"never-copy"}});
        let old_result = outcome(&old);
        let new_result = outcome(&new);
        ledger.finish(new, new_result.clone()).unwrap();
        ledger.finish(old, old_result).unwrap();
        let disk: Value =
            serde_json::from_slice(&fs::read(directory.join("result.json")).unwrap()).unwrap();
        assert_eq!(disk, new_result);
        assert_eq!(ledger.completed.len(), 2);
        assert!(ledger.completed.iter().all(|v| v.get("state").is_none()));
        assert!(ledger.pending.is_empty());
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn public_projection_excludes_legacy_auth_display_and_secrets() {
        let source = json!({"phase":"authorizing","error":"private-response-secret","status":"private-error-chain","accessToken":"secret","config":{"baseUrl":"https://example.test","appId":"id","cloudflaredPath":"private-path"},"authorization":{"verificationUri":"https://login.example.test/activate","userCode":"PUBLIC-CODE","expiresAt":"later","deviceCode":"secret","refreshToken":"secret"},"local":{"deviceId":"device","ports":[{"id":"message","port":1234,"type":"message","transport":"tcp","registrationKey":"secret"}],"privateKey":"secret"},"devices":[],"records":[],"connection":null});
        let mut source = source;
        source["remoteAction"] = json!("off");
        source["localClosed"] = json!(true);
        source["coordinatorRevocation"] = json!("pending");
        source["sdkSnapshot"] = json!({"remoteEnabled":false,"publication":{"phase":"disabled","reason":"remote-disabled","retryable":false,"token":"secret"},"providerError":"private-secret"});
        let value = public_projection(&source);
        assert_eq!(value["localClosed"], true);
        assert_eq!(value["coordinatorRevocation"], "pending");
        assert_eq!(value["sdkSnapshot"]["publication"]["phase"], "disabled");
        assert!(value.get("authorization").is_none());
        assert_eq!(value["local"]["ports"][0]["port"], 1234);
        assert!(value["connection"].is_null());
        let encoded = serde_json::to_string(&value).unwrap();
        assert!(!encoded.contains("secret"));
        assert!(!encoded.contains("private-"));
    }
    #[tokio::test]
    async fn real_lab_dispatch_and_public_snapshot() {
        let directory = root();
        let state = LabState::new(directory.join("data"), directory.join("cache")).unwrap();
        // Exercise real state methods without a network or an OAuth grant.
        assert!(dispatch(&state, Action::Refresh).await.is_err());
        assert!(dispatch(&state, Action::EnableRemote).await.is_err());
        assert!(dispatch(&state, Action::DisableRemote).await.is_err());
        assert!(dispatch(&state, Action::Connect("missing".into()))
            .await
            .is_err());
        assert!(dispatch(&state, Action::Send("message".into()))
            .await
            .is_err());
        assert!(dispatch(&state, Action::Probe("missing".into()))
            .await
            .is_err());
        dispatch(&state, Action::Cancel).await.unwrap();
        let view = safe_view(&state).await;
        assert_eq!(view["phase"], "signed_out");
        assert!(view.get("error").is_none());
        assert!(view.get("status").is_none());
        assert!(view["config"].get("cloudflaredPath").is_none());
        let serialized = serde_json::to_string(&view).unwrap();
        for forbidden in [
            "accessToken",
            "refreshToken",
            "registrationKey",
            "privateKey",
            "deviceCode",
            "sessionKey",
        ] {
            assert!(!serialized.contains(forbidden));
        }
        dispatch(&state, Action::Close).await.unwrap();
        fs::remove_dir_all(directory).unwrap();
    }
    #[tokio::test]
    async fn queue_dispatches_and_closes_after_published_result() {
        let directory = root();
        let state = LabState::new(directory.join("data"), directory.join("cache")).unwrap();
        let ledger = Ledger::open(directory.join("control")).unwrap();
        let bytes = request(&ledger, 1);
        let epoch = ledger.epoch.clone();
        let closed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let signal = closed.clone();
        let task = tokio::spawn(run(ledger, state, move || {
            signal.store(true, std::sync::atomic::Ordering::SeqCst);
        }));
        atomic(&directory.join("control/request.json"), &bytes).unwrap();
        let response = async {
            loop {
                if let Ok(b) = fs::read(directory.join("control/result.json")) {
                    break serde_json::from_slice::<Value>(&b).unwrap();
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        };
        let response = tokio::time::timeout(std::time::Duration::from_secs(3), response)
            .await
            .unwrap();
        assert_eq!(response["code"], "APP_ERROR");
        let metadata: Value =
            serde_json::from_slice(&fs::read(directory.join("control/status.json")).unwrap())
                .unwrap();
        assert_eq!(metadata["highwater"], 1);
        assert!(response.get("error").is_none());
        let close = json!({"version":1,"controlEpoch":epoch,"sequence":2,"requestId":"00000000-0000-4000-8000-000000000002","action":"close","payload":{}});
        while directory.join("control/request.json").exists() {
            tokio::task::yield_now().await;
        }
        atomic(
            &directory.join("control/request.json"),
            &serde_json::to_vec(&close).unwrap(),
        )
        .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap();
        assert!(closed.load(std::sync::atomic::Ordering::SeqCst));
        let response: Value =
            serde_json::from_slice(&fs::read(directory.join("control/result.json")).unwrap())
                .unwrap();
        assert_eq!(response["ok"], true);
        assert_eq!(response["sequence"], 2);
        fs::remove_dir_all(directory).unwrap();
    }
}
