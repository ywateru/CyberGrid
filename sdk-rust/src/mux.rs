//! Bounded smux v1 wire implementation interoperable with xtaci/smux.
//! Header: version, command, LE16 length, LE32 stream ID.
use crate::{
    error::{protocol, transport},
    Result,
};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, DuplexStream},
    sync::{mpsc, Mutex, OwnedSemaphorePermit, Semaphore},
};
use tokio_util::sync::CancellationToken;

pub(crate) trait Io: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> Io for T {}
pub(crate) type BoxIo = Box<dyn Io>;
const MAX_FRAME: usize = 32768;
const MAX_STREAMS: usize = 64;
struct Frame {
    cmd: u8,
    sid: u32,
    data: Vec<u8>,
}
struct Packet {
    data: Vec<u8>,
    _permit: OwnedSemaphorePermit,
}
struct Slot {
    input: mpsc::Sender<Packet>,
    cancel: CancellationToken,
}
pub(crate) struct Mux {
    writer: mpsc::Sender<Frame>,
    slots: Mutex<HashMap<u32, Slot>>,
    accept_rx: Mutex<mpsc::Receiver<DuplexStream>>,
    accept_tx: mpsc::Sender<DuplexStream>,
    next: AtomicU32,
    client: bool,
    pub cancel: CancellationToken,
    budget: Arc<Semaphore>,
    last_frame: Mutex<Instant>,
}
impl Mux {
    pub fn start(io: BoxIo, client: bool, cancel: CancellationToken) -> Arc<Self> {
        let (tx, mut rx) = mpsc::channel::<Frame>(32);
        let (atx, arx) = mpsc::channel(MAX_STREAMS);
        let mux = Arc::new(Self {
            writer: tx,
            slots: Mutex::new(HashMap::new()),
            accept_rx: Mutex::new(arx),
            accept_tx: atx,
            next: AtomicU32::new(if client { 3 } else { 2 }),
            client,
            cancel: cancel.clone(),
            budget: Arc::new(Semaphore::new(3 << 20)),
            last_frame: Mutex::new(Instant::now()),
        });
        let (mut read, mut write) = tokio::io::split(io);
        let c = cancel.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {_=c.cancelled()=>break,frame=rx.recv()=>{
                    let Some(frame)=frame else{break};
                    let mut header=[1,frame.cmd,0,0,0,0,0,0];header[2..4].copy_from_slice(&(frame.data.len()as u16).to_le_bytes());header[4..8].copy_from_slice(&frame.sid.to_le_bytes());
                    let result=tokio::select!{_=c.cancelled()=>break,r=async{write.write_all(&header).await?;write.write_all(&frame.data).await?;write.flush().await}=>r};
                    if result.is_err(){break;}
                }}
            }
            c.cancel();
        });
        let m = mux.clone();
        let c = cancel.clone();
        tokio::spawn(async move {
            let result:Result<()>=async{
                loop{
                    let mut h=[0u8;8];tokio::select!{_=c.cancelled()=>return Ok(()),r=read.read_exact(&mut h)=>{r?;}}
                    if h[0]!=1{return Err(protocol("smux version must be 1"));}
                    let size=u16::from_le_bytes([h[2],h[3]])as usize;let sid=u32::from_le_bytes(h[4..8].try_into().map_err(|_|protocol("invalid stream ID"))?);
                    if size>MAX_FRAME{return Err(protocol("smux frame exceeds limit"));}
                    *m.last_frame.lock().await=Instant::now();
                    match h[1]{
                        0=>{
                            if size!=0||sid==0||sid%2==u32::from(m.client){return Err(protocol("invalid smux SYN parity/length"));}
                            let stream=m.make_stream(sid).await?;
                            tokio::select!{_=c.cancelled()=>return Ok(()),r=m.accept_tx.send(stream)=>{r.map_err(|_|transport("smux accept queue closed"))?;}}
                        },
                        1=>{if size!=0{return Err(protocol("smux FIN has payload"));}m.slots.lock().await.remove(&sid);},
                        2=>{
                            let permit=tokio::select!{_=c.cancelled()=>return Ok(()),p=m.budget.clone().acquire_many_owned(size as u32)=>p.map_err(|_|transport("smux receive budget closed"))?};
                            let mut b=vec![0;size];tokio::select!{_=c.cancelled()=>return Ok(()),r=read.read_exact(&mut b)=>{r?;}}
                            let tx=m.slots.lock().await.get(&sid).map(|s|s.input.clone());
                            if let Some(tx)=tx{tokio::select!{_=c.cancelled()=>return Ok(()),_=tx.send(Packet{data:b,_permit:permit})=>{}}}
                        },
                        3=>{if size!=0{return Err(protocol("smux NOP has payload"));}},
                        _=>return Err(protocol("unknown smux command")),
                    }
                }
            }.await;
            let _ = result;
            c.cancel();
            let slots = std::mem::take(&mut *m.slots.lock().await);
            for (_, s) in slots {
                s.cancel.cancel();
            }
        });
        let m = mux.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(10));
            tick.tick().await;
            loop {
                tokio::select! {_=m.cancel.cancelled()=>break,_=tick.tick()=>{
                    if m.last_frame.lock().await.elapsed()>Duration::from_secs(30){m.cancel.cancel();break;}
                    if m.send(3,0,Vec::new()).await.is_err(){break;}
                }}
            }
        });
        mux
    }
    async fn send(&self, cmd: u8, sid: u32, data: Vec<u8>) -> Result<()> {
        tokio::select! {_=self.cancel.cancelled()=>Err(transport("smux closed")),r=self.writer.send(Frame{cmd,sid,data})=>r.map_err(|_|transport("smux writer closed"))}
    }
    async fn make_stream(self: &Arc<Self>, sid: u32) -> Result<DuplexStream> {
        let (app, worker) = tokio::io::duplex(8192);
        let (input, mut input_rx) = mpsc::channel::<Packet>(4);
        let stream_cancel = self.cancel.child_token();
        {
            let mut slots = self.slots.lock().await;
            if slots.len() >= MAX_STREAMS || slots.contains_key(&sid) {
                return Err(protocol("smux stream capacity or duplicate ID"));
            }
            slots.insert(
                sid,
                Slot {
                    input,
                    cancel: stream_cancel.clone(),
                },
            );
        }
        let (mut r, mut w) = tokio::io::split(worker);
        let c = stream_cancel.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {_=c.cancelled()=>break,p=input_rx.recv()=>{
                    let Some(p)=p else{break};let done=tokio::select!{_=c.cancelled()=>break,r=w.write_all(&p.data)=>r};if done.is_err(){break;}
                }}
            }
            let _ = w.shutdown().await;
        });
        let m = self.clone();
        let c = stream_cancel.clone();
        tokio::spawn(async move {
            let mut b = vec![0; MAX_FRAME];
            loop {
                tokio::select! {_=c.cancelled()=>break,res=r.read(&mut b)=>{
                    match res{Ok(0)|Err(_)=>break,Ok(n)=>{if m.send(2,sid,b[..n].to_vec()).await.is_err(){break;}}}
                }}
            }
            let _ = m.send(1, sid, Vec::new()).await;
            m.slots.lock().await.remove(&sid);
            c.cancel();
        });
        Ok(app)
    }
    pub async fn open(self: &Arc<Self>) -> Result<DuplexStream> {
        let sid = self.next.fetch_add(2, Ordering::SeqCst);
        if sid > u32::MAX - 2 {
            self.cancel.cancel();
            return Err(protocol("smux stream ID exhausted"));
        }
        let stream = self.make_stream(sid).await?;
        self.send(0, sid, Vec::new()).await?;
        Ok(stream)
    }
    pub async fn accept(&self) -> Result<DuplexStream> {
        tokio::select! {_=self.cancel.cancelled()=>Err(transport("smux closed")),s=async{self.accept_rx.lock().await.recv().await}=>s.ok_or_else(||transport("smux accept closed"))}
    }
}
/// Converts the per-stream BE32 data/FIN protocol to a bounded byte stream.
pub(crate) fn framed_stream(raw: DuplexStream, cancel: CancellationToken) -> DuplexStream {
    let (app, worker) = tokio::io::duplex(32768);
    let (mut raw_r, mut raw_w) = tokio::io::split(raw);
    let (mut app_r, mut app_w) = tokio::io::split(worker);
    tokio::spawn(async move {
        let inbound = async {
            loop {
                let n = raw_r.read_u32().await?;
                if n == 0 {
                    app_w.shutdown().await?;
                    return Ok::<_, std::io::Error>(());
                }
                if n > 32768 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "invalid application frame size",
                    ));
                }
                let mut b = vec![0; n as usize];
                raw_r.read_exact(&mut b).await?;
                app_w.write_all(&b).await?;
            }
        };
        let outbound = async {
            let mut b = vec![0; 32768];
            loop {
                let n = app_r.read(&mut b).await?;
                raw_w.write_u32(n as u32).await?;
                if n == 0 {
                    return Ok::<_, std::io::Error>(());
                }
                raw_w.write_all(&b[..n]).await?;
            }
        };
        tokio::pin!(inbound);
        tokio::pin!(outbound);
        tokio::select! {
            _=cancel.cancelled()=>{},
            r=&mut inbound=>{if r.is_ok(){tokio::select!{_=cancel.cancelled()=>{},_=outbound=>{}}}},
            r=&mut outbound=>{if r.is_ok(){tokio::select!{_=cancel.cancelled()=>{},_=inbound=>{}}}},
        }
    });
    app
}
