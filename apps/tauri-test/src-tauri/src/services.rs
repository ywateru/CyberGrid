use crate::protocol::{self, MessageAck, MessageRequest};
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::mpsc,
    task::{JoinHandle, JoinSet},
};
use tokio_util::sync::CancellationToken;

pub struct Incoming {
    pub peer_id: Option<String>,
    pub kind: &'static str,
    pub text: Option<String>,
    pub bytes: Option<usize>,
    pub sha256: Option<String>,
}

pub struct LocalServices {
    pub message_port: u16,
    pub echo_port: u16,
    cancel: CancellationToken,
    tasks: tokio::sync::Mutex<Vec<JoinHandle<()>>>,
}
impl LocalServices {
    pub async fn start(
        device_id: String,
        incoming: mpsc::Sender<Incoming>,
    ) -> Result<Arc<Self>, String> {
        let messages = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| format!("消息服务启动失败：{e}"))?;
        let echo = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| format!("校验服务启动失败：{e}"))?;
        let service = Arc::new(Self {
            message_port: messages.local_addr().map_err(|e| e.to_string())?.port(),
            echo_port: echo.local_addr().map_err(|e| e.to_string())?.port(),
            cancel: CancellationToken::new(),
            tasks: tokio::sync::Mutex::new(Vec::new()),
        });
        let cancel = service.cancel.clone();
        let tx = incoming.clone();
        let messages_task = tokio::spawn(async move {
            let mut children = JoinSet::new();
            loop {
                tokio::select! {
                    _=cancel.cancelled()=>break,
                    Some(_)=children.join_next(), if !children.is_empty()=>{},
                    result=messages.accept()=> {
                        let Ok((mut stream,_))=result else {break};
                        if children.len()>=32 { continue; }
                        let device_id=device_id.clone();let tx=tx.clone();let child_cancel=cancel.clone();
                        children.spawn(async move {
                            tokio::select! { _=child_cancel.cancelled()=>{}, _=tokio::time::timeout(Duration::from_secs(20),async {
                                let request:MessageRequest=protocol::read_json(&mut stream).await?;
                                request.validate()?;
                                let ack=MessageAck{version:1,kind:"ack".into(),id:request.id,receiver_device_id:device_id,sha256:protocol::text_hash(&request.text)};
                                tx.send(Incoming{peer_id:Some(request.from_device_id),kind:"message",text:Some(request.text),bytes:None,sha256:None}).await.map_err(|_|"消息收件服务已停止".to_string())?;
                                protocol::write_json(&mut stream,&ack).await?;
                                stream.shutdown().await.map_err(|e|e.to_string())?;
                                Ok::<(),String>(())
                            })=>{} }
                        });
                    }
                }
            }
            children.abort_all();
            while children.join_next().await.is_some() {}
        });
        let cancel = service.cancel.clone();
        let echo_task = tokio::spawn(async move {
            let mut children = JoinSet::new();
            loop {
                tokio::select! {
                    _=cancel.cancelled()=>break,
                    Some(_)=children.join_next(), if !children.is_empty()=>{},
                    result=echo.accept()=> {
                        let Ok((mut stream,_))=result else {break};
                        if children.len()>=32 { continue; }
                        let tx=incoming.clone();let child_cancel=cancel.clone();
                        children.spawn(async move {
                            tokio::select! { _=child_cancel.cancelled()=>{}, _=tokio::time::timeout(Duration::from_secs(70),async {
                                let mut count=0;let mut hash=Sha256::new();let mut buffer=[0u8;8192];
                                loop { let n=stream.read(&mut buffer).await.map_err(|e|e.to_string())?; if n==0 {break;} count+=n;
                                    if count>protocol::PROBE_BYTES {return Err("校验数据超过64KiB".to_string());}
                                    hash.update(&buffer[..n]);stream.write_all(&buffer[..n]).await.map_err(|e|e.to_string())?;
                                }
                                stream.shutdown().await.map_err(|e|e.to_string())?;
                                if count==protocol::PROBE_BYTES { tx.send(Incoming{peer_id:None,kind:"probe",text:None,bytes:Some(count),sha256:Some(hex::encode(hash.finalize()))}).await.map_err(|_|"校验收件服务已停止".to_string())?; }
                                Ok::<(),String>(())
                            })=>{} }
                        });
                    }
                }
            }
            children.abort_all();
            while children.join_next().await.is_some() {}
        });
        service
            .tasks
            .lock()
            .await
            .extend([messages_task, echo_task]);
        Ok(service)
    }
    pub async fn close(&self) {
        self.cancel.cancel();
        for task in self.tasks.lock().await.drain(..) {
            let _ = task.await;
        }
    }
}

impl Drop for LocalServices {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn two_real_loopback_services_deliver_both_directions_and_verify_echo() {
        let (a_tx, mut a_rx) = mpsc::channel(8);
        let (b_tx, mut b_rx) = mpsc::channel(8);
        let a = LocalServices::start("a".into(), a_tx).await.unwrap();
        let b = LocalServices::start("b".into(), b_tx).await.unwrap();
        protocol::send_message(
            ([127, 0, 0, 1], b.message_port).into(),
            "a",
            "b",
            "Mac to Android <hello>",
        )
        .await
        .unwrap();
        let received = b_rx.recv().await.unwrap();
        assert_eq!(received.peer_id.as_deref(), Some("a"));
        assert_eq!(received.text.as_deref(), Some("Mac to Android <hello>"));
        protocol::send_message(
            ([127, 0, 0, 1], a.message_port).into(),
            "b",
            "a",
            "Android to Mac 你好",
        )
        .await
        .unwrap();
        let received = a_rx.recv().await.unwrap();
        assert_eq!(received.peer_id.as_deref(), Some("b"));
        assert_eq!(received.text.as_deref(), Some("Android to Mac 你好"));
        let (bytes, hash) = protocol::probe(([127, 0, 0, 1], b.echo_port).into())
            .await
            .unwrap();
        assert_eq!(bytes, 65_536);
        let received = b_rx.recv().await.unwrap();
        assert_eq!(received.bytes, Some(65_536));
        assert_eq!(received.sha256.as_deref(), Some(hash.as_str()));
        let a_port = a.message_port;
        a.close().await;
        b.close().await;
        assert!(
            tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, a_port))
                .await
                .is_err()
        );
    }
}
