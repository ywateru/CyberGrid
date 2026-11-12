use crate::{
    client::Client,
    error::{protocol, transport},
    mux::{BoxIo, Mux},
    Device, Error, Peer, Result,
};
use rcgen::{
    CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, KeyPair,
    KeyUsagePurpose, PKCS_ED25519,
};
use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime},
    server::danger::{ClientCertVerified, ClientCertVerifier},
    DigitallySignedStruct, SignatureScheme,
};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio_rustls::{TlsAcceptor, TlsConnector};

pub(crate) struct Identity {
    pub cert: CertificateDer<'static>,
    key: Vec<u8>,
    pub fingerprint: String,
}
impl Identity {
    pub fn new(id: &str) -> Result<Self> {
        let key = KeyPair::generate_for(&PKCS_ED25519)
            .map_err(|_| transport("cannot generate device key"))?;
        let mut params = CertificateParams::new(Vec::<String>::new())
            .map_err(|_| transport("invalid certificate parameters"))?;
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, id);
        params.distinguished_name = dn;
        params.not_before = time::OffsetDateTime::now_utc() - time::Duration::minutes(1);
        params.not_after = time::OffsetDateTime::now_utc() + time::Duration::hours(24);
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![
            ExtendedKeyUsagePurpose::ClientAuth,
            ExtendedKeyUsagePurpose::ServerAuth,
        ];
        let cert = params
            .self_signed(&key)
            .map_err(|_| transport("cannot sign device certificate"))?
            .der()
            .clone();
        Ok(Self {
            fingerprint: hex::encode(Sha256::digest(cert.as_ref())),
            cert,
            key: key.serialize_der(),
        })
    }
    fn key(&self) -> PrivateKeyDer<'static> {
        PrivatePkcs8KeyDer::from(self.key.clone()).into()
    }
}
pub(crate) fn certificate_id(der: &[u8]) -> Result<String> {
    let (rest, cert) = x509_parser::parse_x509_certificate(der)
        .map_err(|_| protocol("invalid peer certificate"))?;
    if !rest.is_empty() {
        return Err(protocol("trailing certificate bytes"));
    }
    let mut cn = cert.subject().iter_common_name();
    let first = cn
        .next()
        .ok_or_else(|| protocol("peer certificate missing CN"))?;
    if cn.next().is_some() {
        return Err(protocol("multiple peer CN values"));
    }
    let id = first
        .as_str()
        .map_err(|_| protocol("invalid certificate CN"))?;
    if id.is_empty() || id.len() > 128 {
        return Err(protocol("invalid device certificate ID"));
    }
    Ok(id.into())
}
pub(crate) fn check_certificate(der: &[u8], id: &str, pin: &str, now: u64) -> Result<()> {
    if certificate_id(der)? != id {
        return Err(protocol("peer certificate device ID mismatch"));
    }
    let (_, cert) = x509_parser::parse_x509_certificate(der)
        .map_err(|_| protocol("invalid peer certificate"))?;
    if cert.public_key().algorithm.algorithm.to_id_string() != "1.3.101.112"
        || cert.signature_algorithm.algorithm.to_id_string() != "1.3.101.112"
    {
        return Err(protocol("peer must use Ed25519 certificate"));
    }
    if (now as i64) < cert.validity().not_before.timestamp()
        || (now as i64) > cert.validity().not_after.timestamp()
    {
        return Err(protocol("peer certificate expired or not yet valid"));
    }
    if pin.len() != 64 || !hex::encode(Sha256::digest(der)).eq_ignore_ascii_case(pin) {
        return Err(protocol("peer certificate fingerprint mismatch"));
    }
    Ok(())
}
#[derive(Debug)]
struct Pins {
    pins: HashMap<String, String>,
}
impl Pins {
    fn check(
        &self,
        leaf: &CertificateDer<'_>,
        chain: &[CertificateDer<'_>],
        now: UnixTime,
    ) -> std::result::Result<(), rustls::Error> {
        if !chain.is_empty() {
            return Err(rustls::Error::General(
                "expected exactly one device certificate".into(),
            ));
        }
        let id = certificate_id(leaf.as_ref())
            .map_err(|_| rustls::Error::General("invalid device certificate".into()))?;
        let pin = self.pins.get(&id).ok_or_else(|| {
            rustls::Error::General("device not in current authenticated namespace".into())
        })?;
        check_certificate(leaf.as_ref(), &id, pin, now.as_secs()).map_err(|_| {
            rustls::Error::General("device certificate identity/pin/validity rejected".into())
        })
    }
    fn signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }
}
impl ServerCertVerifier for Pins {
    fn verify_server_cert(
        &self,
        cert: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        self.check(cert, intermediates, now)?;
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::General("TLS1.2 disabled".into()))
    }
    fn verify_tls13_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        s: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        self.signature(m, c, s)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ED25519]
    }
}
impl ClientCertVerifier for Pins {
    fn root_hint_subjects(&self) -> &[rustls::DistinguishedName] {
        &[]
    }
    fn verify_client_cert(
        &self,
        c: &CertificateDer<'_>,
        i: &[CertificateDer<'_>],
        n: UnixTime,
    ) -> std::result::Result<ClientCertVerified, rustls::Error> {
        self.check(c, i, n)?;
        Ok(ClientCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::General("TLS1.2 disabled".into()))
    }
    fn verify_tls13_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        s: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        self.signature(m, c, s)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ED25519]
    }
}
impl Client {
    pub(crate) async fn secure_client(
        self: &Arc<Self>,
        raw: BoxIo,
        device: &Device,
        path: &str,
    ) -> Result<Arc<Peer>> {
        self.secure_client_fenced(raw, device, path, None).await
    }
    pub(crate) async fn secure_client_fenced(
        self: &Arc<Self>,
        raw: BoxIo,
        device: &Device,
        path: &str,
        fence: Option<&crate::peer::DialFence>,
    ) -> Result<Arc<Peer>> {
        let rt = self.active().await?;
        if fence.is_some_and(|f| f.generation != rt.generation || f.cancel.is_cancelled()) {
            return Err(Error::Cancelled);
        }
        let pin = device
            .certificate_fingerprint
            .clone()
            .ok_or_else(|| protocol("peer missing certificate fingerprint"))?;
        let config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|_| transport("TLS configuration failed"))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Pins {
            pins: HashMap::from([(device.id.clone(), pin)]),
        }))
        .with_client_auth_cert(vec![rt.identity.cert.clone()], rt.identity.key())
        .map_err(|_| transport("client certificate invalid"))?;
        let tls = tokio::select! {
            _ = rt.cancel.cancelled() => return Err(Error::Cancelled),
            tls = tokio::time::timeout(Duration::from_secs(10),
                TlsConnector::from(Arc::new(config)).connect(
                    ServerName::try_from("cybergrid.invalid")
                        .map_err(|_| protocol("invalid TLS name"))?, raw)) => tls,
        }
        .map_err(|_| Error::Timeout("peer TLS handshake"))?
        .map_err(|_| transport("peer TLS authentication rejected"))?;
        let cert = tls
            .get_ref()
            .1
            .peer_certificates()
            .and_then(|c| c.first())
            .ok_or_else(|| protocol("TLS peer missing certificate"))?;
        let fresh = self.device(&device.id).await?;
        if fresh.incarnation != device.incarnation
            || (path == "iroh-direct" && fresh.iroh != device.iroh)
        {
            return Err(protocol(
                "peer transport publication changed during handshake",
            ));
        }
        check_certificate(
            cert.as_ref(),
            &fresh.id,
            fresh
                .certificate_fingerprint
                .as_deref()
                .ok_or_else(|| protocol("peer certificate withdrawn"))?,
            UnixTime::now().as_secs(),
        )?;
        if rt.cancel.is_cancelled() || self.generation() != rt.generation {
            return Err(Error::Cancelled);
        }
        if let Some(fence) = fence {
            fence.claim()?;
        }
        self.event("mutual-tls-ready", Some(path), Some(&device.id), None);
        let mux = Mux::start(Box::new(tls), true, rt.cancel.child_token());
        self.remember_peer(device.id.clone(), path.into(), mux, rt.generation, fence)
            .await
    }
    pub(crate) async fn secure_server(
        self: &Arc<Self>,
        raw: BoxIo,
        path: &str,
    ) -> Result<Arc<Peer>> {
        self.secure_server_bound(raw, path, None, None).await
    }
    pub(crate) async fn secure_server_bound(
        self: &Arc<Self>,
        raw: BoxIo,
        path: &str,
        expected: Option<&Device>,
        generation: Option<u64>,
    ) -> Result<Arc<Peer>> {
        self.require_remote_access()?;
        let rt = self.active().await?;
        if generation.is_some_and(|generation| generation != rt.generation) {
            return Err(Error::Cancelled);
        }
        let pins = self
            .devices()
            .await?
            .into_iter()
            .filter(|d| d.online)
            .filter(|d| {
                expected.is_none_or(|expected| {
                    expected.id == d.id
                        && expected.incarnation == d.incarnation
                        && expected.iroh == d.iroh
                })
            })
            .filter_map(|d| d.certificate_fingerprint.map(|fp| (d.id, fp)))
            .collect();
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|_| transport("TLS configuration failed"))?
        .with_client_cert_verifier(Arc::new(Pins { pins }))
        .with_single_cert(vec![rt.identity.cert.clone()], rt.identity.key())
        .map_err(|_| transport("server certificate invalid"))?;
        let tls = tokio::select! {
            _ = rt.cancel.cancelled() => return Err(Error::Cancelled),
            tls = tokio::time::timeout(Duration::from_secs(10),
                TlsAcceptor::from(Arc::new(config)).accept(raw)) => tls,
        }
        .map_err(|_| Error::Timeout("peer TLS handshake"))?
        .map_err(|_| transport("peer TLS authentication rejected"))?;
        let cert = tls
            .get_ref()
            .1
            .peer_certificates()
            .and_then(|c| c.first())
            .ok_or_else(|| protocol("TLS peer missing certificate"))?;
        let id = certificate_id(cert.as_ref())?;
        let fresh = self.device(&id).await?;
        if expected.is_some_and(|d| {
            d.id != id || d.incarnation != fresh.incarnation || d.iroh != fresh.iroh
        }) {
            return Err(protocol(
                "iroh endpoint no longer bound to authenticated device",
            ));
        }
        check_certificate(
            cert.as_ref(),
            &id,
            fresh
                .certificate_fingerprint
                .as_deref()
                .ok_or_else(|| protocol("peer certificate withdrawn"))?,
            UnixTime::now().as_secs(),
        )?;
        if rt.cancel.is_cancelled() || self.generation() != rt.generation {
            return Err(Error::Cancelled);
        }
        self.require_remote_access()?;
        self.event("mutual-tls-ready", Some(path), Some(&id), None);
        let mux = Mux::start(Box::new(tls), false, rt.cancel.child_token());
        self.remember_peer(id, path.into(), mux, rt.generation, None)
            .await
    }
}
