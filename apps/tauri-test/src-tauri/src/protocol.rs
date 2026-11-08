use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const MESSAGE_PORT: &str = "lab-message";
pub const ECHO_PORT: &str = "lab-echo";
pub const MAX_TEXT_BYTES: usize = 4096;
const MAX_FRAME_BYTES: usize = 8192;
pub const PROBE_BYTES: usize = 65_536;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MessageRequest {
    pub version: u8,
    #[serde(rename = "type")]
    pub kind: String,
    pub id: String,
    pub from_device_id: String,
    pub text: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MessageAck {
    pub version: u8,
    #[serde(rename = "type")]
    pub kind: String,
    pub id: String,
    pub receiver_device_id: String,
    pub sha256: String,
}

impl MessageRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.kind != "message" {
            return Err("不支持的消息协议".into());
        }
        if self.id.len() != 32 || !self.id.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err("消息 ID 无效".into());
        }
        if self.from_device_id.is_empty() || self.from_device_id.len() > 80 {
            return Err("发送设备 ID 无效".into());
        }
        if self.text.trim().is_empty() || self.text.len() > MAX_TEXT_BYTES {
            return Err("消息必须为 1 至 4096 字节 UTF-8 文本".into());
        }
        Ok(())
    }
}

pub fn text_hash(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}
pub fn bytes_hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub async fn read_json<T: for<'de> Deserialize<'de>, R: AsyncRead + Unpin>(
    read: &mut R,
) -> Result<T, String> {
    let len = read
        .read_u32()
        .await
        .map_err(|e| format!("无法读取消息长度：{e}"))? as usize;
    if len == 0 || len > MAX_FRAME_BYTES {
        return Err("消息帧长度超出范围".into());
    }
    let mut bytes = vec![0; len];
    read.read_exact(&mut bytes)
        .await
        .map_err(|e| format!("消息不完整：{e}"))?;
    serde_json::from_slice(&bytes).map_err(|_| "消息格式无效".into())
}

pub async fn write_json<T: Serialize, W: AsyncWrite + Unpin>(
    write: &mut W,
    value: &T,
) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|_| "消息编码失败".to_string())?;
    if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
        return Err("消息帧长度超出范围".into());
    }
    write
        .write_u32(bytes.len() as u32)
        .await
        .map_err(|e| format!("无法发送消息：{e}"))?;
    write
        .write_all(&bytes)
        .await
        .map_err(|e| format!("无法发送消息：{e}"))?;
    write
        .flush()
        .await
        .map_err(|e| format!("无法发送消息：{e}"))
}

pub async fn send_message(
    address: std::net::SocketAddr,
    sender: &str,
    receiver: &str,
    text: &str,
) -> Result<(), String> {
    let request = MessageRequest {
        version: 1,
        kind: "message".into(),
        id: hex::encode(rand::random::<[u8; 16]>()),
        from_device_id: sender.into(),
        text: text.into(),
    };
    request.validate()?;
    tokio::time::timeout(Duration::from_secs(25), async {
        let mut stream = tokio::net::TcpStream::connect(address)
            .await
            .map_err(|e| format!("无法连接本地映射：{e}"))?;
        write_json(&mut stream, &request).await?;
        stream
            .shutdown()
            .await
            .map_err(|e| format!("无法完成消息发送：{e}"))?;
        let ack: MessageAck = read_json(&mut stream).await?;
        if ack.version != 1
            || ack.kind != "ack"
            || ack.id != request.id
            || ack.receiver_device_id != receiver
            || ack.sha256 != text_hash(text)
        {
            return Err("对端确认不匹配，消息未确认送达".into());
        }
        Ok(())
    })
    .await
    .map_err(|_| "等待对端确认超时，消息未确认送达".to_string())?
}

pub async fn probe(address: std::net::SocketAddr) -> Result<(usize, String), String> {
    let payload: Vec<u8> = (0..PROBE_BYTES).map(|_| rand::random::<u8>()).collect();
    let expected = bytes_hash(&payload);
    let echoed = tokio::time::timeout(Duration::from_secs(60), async {
        let stream = tokio::net::TcpStream::connect(address)
            .await
            .map_err(|e| format!("无法连接校验端口：{e}"))?;
        let (mut read, mut write) = stream.into_split();
        let writer = tokio::spawn(async move {
            write
                .write_all(&payload)
                .await
                .map_err(|e| format!("数据发送失败：{e}"))?;
            write
                .shutdown()
                .await
                .map_err(|e| format!("数据半关闭失败：{e}"))
        });
        let mut echoed = Vec::with_capacity(PROBE_BYTES);
        let read_result = (&mut read)
            .take((PROBE_BYTES + 1) as u64)
            .read_to_end(&mut echoed)
            .await;
        if let Err(e) = read_result {
            writer.abort();
            return Err(format!("数据接收失败：{e}"));
        }
        writer.await.map_err(|_| "数据发送任务中断".to_string())??;
        Ok(echoed)
    })
    .await
    .map_err(|_| "64 KiB 传输校验超时".to_string())??;
    if echoed.len() != PROBE_BYTES || bytes_hash(&echoed) != expected {
        return Err("传输字节数或 SHA256 不一致".into());
    }
    Ok((echoed.len(), expected))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_protocol_and_utf8_byte_overflow() {
        let mut request = MessageRequest {
            version: 1,
            kind: "message".into(),
            id: "a".repeat(32),
            from_device_id: "device".into(),
            text: "你好".into(),
        };
        assert!(request.validate().is_ok());
        request.text = "你".repeat(1366);
        assert!(request.validate().is_err());
        request.text = "ok".into();
        request.version = 2;
        assert!(request.validate().is_err());
    }
    #[tokio::test]
    async fn bounded_frame_rejects_oversized_before_allocation() {
        let (mut a, mut b) = tokio::io::duplex(64);
        a.write_u32(8193).await.unwrap();
        assert!(read_json::<MessageRequest, _>(&mut b)
            .await
            .err()
            .unwrap()
            .contains("长度"));
    }
    #[tokio::test]
    async fn actual_tcp_message_requires_matching_receiver_ack() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request: MessageRequest = read_json(&mut stream).await.unwrap();
            write_json(
                &mut stream,
                &MessageAck {
                    version: 1,
                    kind: "ack".into(),
                    id: request.id,
                    receiver_device_id: "wrong-device".into(),
                    sha256: text_hash(&request.text),
                },
            )
            .await
            .unwrap();
        });
        assert!(send_message(addr, "local", "expected-device", "hello")
            .await
            .unwrap_err()
            .contains("不匹配"));
        server.await.unwrap();
    }
}
