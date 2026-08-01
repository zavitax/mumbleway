//! TLS setup for the control channel.
//!
//! Two things make Mumble different from ordinary HTTPS:
//!
//! 1. **Servers are self-signed.** Virtually no Mumble server presents a WebPKI
//!    chain, so strict validation would reject nearly every real server. We use
//!    trust-on-first-use instead: remember the certificate fingerprint the first
//!    time we see a host, and refuse to connect if it later changes unless the
//!    user explicitly re-pins it.
//! 2. **Clients are identified by certificate.** Server-side user registration
//!    keys off a client certificate, so we generate a stable self-signed identity
//!    once and reuse it for every connection.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use sha2::{Digest, Sha256};

use crate::error::{CoreError, Result};

/// What to do about the server's certificate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustPolicy {
    /// First contact — accept whatever is presented and report the fingerprint.
    TrustOnFirstUse,
    /// We have seen this host before and expect this exact fingerprint.
    Pinned(String),
    /// The user chose to accept a changed certificate for this session.
    AcceptAny,
}

/// Records what the server actually presented, so the caller can persist or
/// surface it after a successful handshake.
#[derive(Debug, Default)]
pub struct ObservedCert {
    pub fingerprint: parking_lot::Mutex<Option<String>>,
    pub mismatch: parking_lot::Mutex<Option<String>>,
}

impl ObservedCert {
    pub fn fingerprint(&self) -> Option<String> {
        self.fingerprint.lock().clone()
    }
    pub fn mismatch(&self) -> Option<String> {
        self.mismatch.lock().clone()
    }
}

/// SHA-256 fingerprint of a DER certificate, lowercase hex.
pub fn fingerprint_of(der: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(der);
    hex::encode(h.finalize())
}

#[derive(Debug)]
struct TofuVerifier {
    policy: TrustPolicy,
    observed: Arc<ObservedCert>,
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl ServerCertVerifier for TofuVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        let fp = fingerprint_of(end_entity.as_ref());
        *self.observed.fingerprint.lock() = Some(fp.clone());

        match &self.policy {
            TrustPolicy::TrustOnFirstUse | TrustPolicy::AcceptAny => {
                Ok(ServerCertVerified::assertion())
            }
            TrustPolicy::Pinned(expected) => {
                if expected.eq_ignore_ascii_case(&fp) {
                    Ok(ServerCertVerified::assertion())
                } else {
                    // Surface the new fingerprint so the UI can show a comparison
                    // and offer an explicit re-pin.
                    *self.observed.mismatch.lock() = Some(fp);
                    Err(rustls::Error::General(
                        "server certificate does not match the pinned fingerprint".into(),
                    ))
                }
            }
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        // The certificate is trusted by fingerprint, but the handshake signature
        // itself must still be cryptographically valid or the channel is worthless.
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// A persistent client identity certificate.
#[derive(Debug, Clone)]
pub struct Identity {
    pub cert_pem: String,
    pub key_pem: String,
}

impl Identity {
    /// Generates a fresh self-signed identity.
    pub fn generate(common_name: &str) -> Result<Self> {
        let mut params = rcgen::CertificateParams::new(vec![common_name.to_string()])
            .map_err(|e| CoreError::Tls(format!("certificate params: {e}")))?;
        let mut dn = rcgen::DistinguishedName::new();
        dn.push(rcgen::DnType::CommonName, common_name);
        params.distinguished_name = dn;

        let key_pair = rcgen::KeyPair::generate()
            .map_err(|e| CoreError::Tls(format!("key generation: {e}")))?;
        let cert = params
            .self_signed(&key_pair)
            .map_err(|e| CoreError::Tls(format!("self-signing: {e}")))?;

        Ok(Self {
            cert_pem: cert.pem(),
            key_pem: key_pair.serialize_pem(),
        })
    }

    /// Loads the identity from `dir`, generating and persisting one if absent.
    ///
    /// The identity is what a Mumble server ties registration to, so losing it
    /// means losing registered-user status — it is created once and kept.
    pub fn load_or_create(dir: &std::path::Path, common_name: &str) -> Result<Self> {
        let cert_path = dir.join("identity.crt");
        let key_path = dir.join("identity.key");

        if cert_path.exists() && key_path.exists() {
            let cert_pem = std::fs::read_to_string(&cert_path)?;
            let key_pem = std::fs::read_to_string(&key_path)?;
            if !cert_pem.trim().is_empty() && !key_pem.trim().is_empty() {
                return Ok(Self { cert_pem, key_pem });
            }
        }

        std::fs::create_dir_all(dir)?;
        let id = Self::generate(common_name)?;
        std::fs::write(&cert_path, &id.cert_pem)?;
        std::fs::write(&key_path, &id.key_pem)?;
        Ok(id)
    }

    fn to_rustls(&self) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
        let certs = rustls_pemfile::certs(&mut self.cert_pem.as_bytes())
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| CoreError::Tls(format!("reading identity certificate: {e}")))?;
        if certs.is_empty() {
            return Err(CoreError::Tls("identity certificate was empty".into()));
        }
        let key = rustls_pemfile::private_key(&mut self.key_pem.as_bytes())
            .map_err(|e| CoreError::Tls(format!("reading identity key: {e}")))?
            .ok_or_else(|| CoreError::Tls("identity key was empty".into()))?;
        Ok((certs, key))
    }
}

/// Builds a `ClientConfig` for a Mumble control connection.
pub fn client_config(
    identity: Option<&Identity>,
    policy: TrustPolicy,
) -> Result<(Arc<ClientConfig>, Arc<ObservedCert>)> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let observed = Arc::new(ObservedCert::default());

    let verifier = Arc::new(TofuVerifier {
        policy,
        observed: observed.clone(),
        provider: provider.clone(),
    });

    let builder = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| CoreError::Tls(format!("protocol versions: {e}")))?
        .dangerous()
        .with_custom_certificate_verifier(verifier);

    let config = match identity {
        Some(id) => {
            let (certs, key) = id.to_rustls()?;
            builder
                .with_client_auth_cert(certs, key)
                .map_err(|e| CoreError::Tls(format!("client auth certificate: {e}")))?
        }
        None => builder.with_no_client_auth(),
    };

    Ok((Arc::new(config), observed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_a_usable_identity() {
        let id = Identity::generate("MumbleWay Test").unwrap();
        assert!(id.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(id.key_pem.contains("PRIVATE KEY"));
        // It must convert into something rustls will accept.
        let (certs, _key) = id.to_rustls().unwrap();
        assert_eq!(certs.len(), 1);
    }

    #[test]
    fn identity_is_stable_across_loads() {
        let dir = std::env::temp_dir().join(format!("mumbleway-id-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let a = Identity::load_or_create(&dir, "MumbleWay").unwrap();
        let b = Identity::load_or_create(&dir, "MumbleWay").unwrap();
        assert_eq!(
            a.cert_pem, b.cert_pem,
            "identity must persist, or server-side registration breaks"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn fingerprints_are_stable_and_distinguishing() {
        let a = Identity::generate("a").unwrap();
        let b = Identity::generate("b").unwrap();
        let (ca, _) = a.to_rustls().unwrap();
        let (cb, _) = b.to_rustls().unwrap();

        let fa = fingerprint_of(ca[0].as_ref());
        assert_eq!(fa.len(), 64, "SHA-256 hex is 64 characters");
        assert_eq!(fa, fingerprint_of(ca[0].as_ref()), "must be deterministic");
        assert_ne!(fa, fingerprint_of(cb[0].as_ref()));
    }

    #[test]
    fn builds_configs_with_and_without_an_identity() {
        let id = Identity::generate("MumbleWay").unwrap();
        assert!(client_config(Some(&id), TrustPolicy::TrustOnFirstUse).is_ok());
        assert!(client_config(None, TrustPolicy::AcceptAny).is_ok());
        assert!(client_config(None, TrustPolicy::Pinned("aa".repeat(32))).is_ok());
    }
}
