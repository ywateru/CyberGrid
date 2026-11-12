use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::{Mutex, Notify},
};
use tokio_util::sync::CancellationToken;
pub const REDIRECT: &str = "http://127.0.0.1:17896/callback";
pub struct Callback {
    value: Mutex<Option<Result<String, &'static str>>>,
    done: Notify,
    stop: CancellationToken,
}
impl Callback {
    pub async fn start() -> Result<Arc<Self>, &'static str> {
        let listener = TcpListener::bind("127.0.0.1:17896")
            .await
            .map_err(|_| "CALLBACK_PORT_BUSY")?;
        let owner = Arc::new(Self {
            value: Mutex::new(None),
            done: Notify::new(),
            stop: CancellationToken::new(),
        });
        let task = owner.clone();
        tokio::spawn(async move {
            let result = tokio::select! {_=task.stop.cancelled()=>Err("CANCELLED"),r=tokio::time::timeout(Duration::from_secs(600),receive(listener, "127.0.0.1:17896"))=>r.unwrap_or(Err("CALLBACK_TIMEOUT"))};
            *task.value.lock().await = Some(result);
            task.done.notify_waiters();
        });
        Ok(owner)
    }
    pub async fn wait(&self) -> Result<String, &'static str> {
        loop {
            let n = self.done.notified();
            if let Some(v) = self.value.lock().await.clone() {
                return v;
            };
            n.await
        }
    }
    pub fn cancel(&self) {
        self.stop.cancel();
    }
}
async fn receive(listener: TcpListener, authority: &str) -> Result<String, &'static str> {
    for _ in 0..16 {
        let (mut socket, _) = listener.accept().await.map_err(|_| "CALLBACK_FAILED")?;
        let mut bytes = Vec::new();
        let result = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let mut b = [0; 1024];
                let n = socket.read(&mut b).await.map_err(|_| ())?;
                if n == 0 || bytes.len() + n > 16384 {
                    return Err(());
                };
                bytes.extend_from_slice(&b[..n]);
                if bytes.windows(4).any(|b| b == b"\r\n\r\n") {
                    return Ok(());
                }
            }
        })
        .await;
        let text = String::from_utf8_lossy(&bytes);
        let mut lines = text.lines();
        let first = lines.next().unwrap_or("");
        let p: Vec<_> = first.split(' ').collect();
        let headers: Vec<_> = lines.take_while(|l| !l.is_empty()).collect();
        let hosts: Vec<_> = headers
            .iter()
            .filter_map(|l| l.split_once(':'))
            .filter(|(key, _)| key.eq_ignore_ascii_case("host"))
            .map(|(_, value)| value.trim())
            .collect();
        let valid = result.is_ok_and(|r| r.is_ok())
            && p.len() == 3
            && p[0] == "GET"
            && p[1].starts_with("/callback?")
            && p[2] == "HTTP/1.1"
            && hosts == vec![authority]
            && headers.iter().all(|line| {
                line.split_once(':').is_some_and(|(key, _)| {
                    !key.eq_ignore_ascii_case("forwarded")
                        && !key.to_ascii_lowercase().starts_with("x-forwarded-")
                })
            });
        let body = if valid {
            "请返回 CyberGrid Workspace。"
        } else {
            "Invalid callback"
        };
        let status = if valid { "200 OK" } else { "400 Bad Request" };
        let response = format!("HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; frame-ancestors 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
        let _ = tokio::time::timeout(
            Duration::from_secs(1),
            socket.write_all(response.as_bytes()),
        )
        .await;
        if valid {
            return Ok(format!("http://{authority}{}", p[1]));
        }
    }
    Err("CALLBACK_FAILED")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn actual_tcp_callback_rejects_rebinding_duplicate_and_forwarded_host() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let authority = address.to_string();
        let expected = format!("http://{authority}/callback?code=synthetic&state=synthetic");
        let task = tokio::spawn(async move { receive(listener, &authority).await });
        for (headers, accepted) in [
            ("Host: attacker.test".to_owned(), false),
            (format!("Host: {address}\r\nHost: {address}"), false),
            (
                format!("Host: {address}\r\nX-Forwarded-Host: attacker.test"),
                false,
            ),
            (format!("host: {address}"), true),
        ] {
            let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
            socket.write_all(format!("GET /callback?code=synthetic&state=synthetic HTTP/1.1\r\n{headers}\r\n\r\n").as_bytes()).await.unwrap();
            let mut response = String::new();
            tokio::time::timeout(Duration::from_secs(1), socket.read_to_string(&mut response))
                .await
                .unwrap()
                .unwrap();
            assert!(response.starts_with(if accepted {
                "HTTP/1.1 200"
            } else {
                "HTTP/1.1 400"
            }));
            assert!(response.contains("Cache-Control: no-store"));
        }
        assert_eq!(task.await.unwrap().unwrap(), expected);
    }
}
