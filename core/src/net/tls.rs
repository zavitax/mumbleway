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
    ///
    /// Three things this used to get wrong, each of which cost a rider either
    /// the identity or every connection:
    ///
    /// - **A pair that does not load was returned anyway.** A truncated or
    ///   garbled key passed the "not empty" check, and every connection after
    ///   that failed in the TLS setup with no way out short of reinstalling.
    ///   The pair is now checked the way a connection would use it — parsed,
    ///   and the key matched against the certificate — before it is trusted.
    /// - **A broken pair was overwritten.** With one half missing or empty, the
    ///   other half was replaced in place, so whatever was left of the old
    ///   identity was gone. Unusable files are now moved aside under a
    ///   `.broken-<unix seconds>` suffix before a new pair is written, so
    ///   nothing that might be recovered by hand is ever destroyed.
    /// - **The files were written in place, readable by everyone.** A crash or
    ///   power loss mid-write leaves a truncated file, and `std::fs::write`
    ///   creates the key with the umask's permissions (0644 on most desktops).
    ///   Each file is now written to a temporary name, flushed to disk and
    ///   renamed over, and on Unix the key is created 0600.
    pub fn load_or_create(dir: &std::path::Path, common_name: &str) -> Result<Self> {
        let cert_path = dir.join(IDENTITY_CERT_FILE);
        let key_path = dir.join(IDENTITY_KEY_FILE);

        let cert = read_if_present(&cert_path)?;
        let key = read_if_present(&key_path)?;
        if let (Some(Some(cert_pem)), Some(Some(key_pem))) = (&cert, &key) {
            let id = Self {
                cert_pem: cert_pem.clone(),
                key_pem: key_pem.clone(),
            };
            match id.check() {
                Ok(()) => {
                    restrict_to_owner(&key_path);
                    return Ok(id);
                }
                Err(e) => tracing::warn!("client identity in {} is unusable: {e}", dir.display()),
            }
        } else if cert.is_some() || key.is_some() {
            tracing::warn!(
                "client identity in {} is incomplete; replacing it",
                dir.display()
            );
        }

        // Whatever is there cannot be used. Keep it, under another name.
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        for (path, present) in [(&cert_path, cert.is_some()), (&key_path, key.is_some())] {
            if present {
                let aside = broken_name(path, stamp);
                std::fs::rename(path, &aside)?;
                tracing::warn!("kept the unusable file as {}", aside.display());
            }
        }

        std::fs::create_dir_all(dir)?;
        let id = Self::generate(common_name)?;
        // The key goes first: a pair is only loaded when both files are there,
        // so a crash between the two renames leaves a lone key, which the next
        // start moves aside like any other incomplete pair.
        write_atomically(&key_path, id.key_pem.as_bytes(), true)?;
        write_atomically(&cert_path, id.cert_pem.as_bytes(), false)?;
        Ok(id)
    }

    /// Fails if a connection could not present this identity: either half
    /// does not parse, or the key does not belong to the certificate.
    fn check(&self) -> Result<()> {
        client_config(Some(self), TrustPolicy::AcceptAny).map(|_| ())
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

const IDENTITY_CERT_FILE: &str = "identity.crt";
const IDENTITY_KEY_FILE: &str = "identity.key";

/// `Ok(None)` when the file is absent, `Ok(Some(None))` when it is there but
/// is not usable text (empty, or not UTF-8), and an error only when it could
/// not be read at all — a permission problem must not look like a missing
/// identity, or the next step would replace one that is merely unreadable.
fn read_if_present(path: &std::path::Path) -> Result<Option<Option<String>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(
            String::from_utf8(bytes)
                .ok()
                .filter(|text| !text.trim().is_empty()),
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn broken_name(path: &std::path::Path, stamp: u64) -> std::path::PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".broken-{stamp}"));
    let mut aside = path.with_file_name(&name);
    // Two failures in the same second must not overwrite the first one kept.
    let mut n = 1;
    while aside.exists() {
        let mut numbered = name.clone();
        numbered.push(format!("-{n}"));
        aside = path.with_file_name(numbered);
        n += 1;
    }
    aside
}

/// Writes `bytes` to a temporary file beside `path`, flushes it to disk and
/// renames it over `path`, so a reader sees the old file or the whole new one
/// and never a truncated one. `secret` creates the file readable by its owner
/// only, on the platforms where that is a file mode.
fn write_atomically(path: &std::path::Path, bytes: &[u8], secret: bool) -> Result<()> {
    use std::io::Write;

    let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    // A mode applies only when the file is created, so a temporary left 0644
    // by an interrupted write must go first or the key would inherit it.
    match std::fs::remove_file(&tmp) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
        _ => {}
    }

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    if secret {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    #[cfg(not(unix))]
    let _ = secret;

    let mut file = options.open(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path)?;

    // The rename itself is only durable once the directory is flushed. Not
    // every platform can open a directory for that, so this is best effort.
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        if let Ok(dir) = std::fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }
    Ok(())
}

/// Tightens an existing key written by an older build with the umask's
/// permissions. Best effort: a key that works is not discarded over its mode.
fn restrict_to_owner(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            if meta.permissions().mode() & 0o077 != 0 {
                let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
            }
        }
    }
    #[cfg(not(unix))]
    let _ = path;
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

    /// A fresh directory per test, so tests running in parallel never share
    /// an identity.
    fn scratch_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mumbleway-id-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Every file in `dir` whose name starts with `prefix`, sorted.
    fn files_starting(dir: &std::path::Path, prefix: &str) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(prefix))
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_truncated_key_is_kept_aside_and_replaced() {
        let dir = scratch_dir("truncated");
        let first = Identity::load_or_create(&dir, "MumbleWay").unwrap();
        let half = &first.key_pem[..first.key_pem.len() / 2];
        std::fs::write(dir.join("identity.key"), half).unwrap();

        let second = Identity::load_or_create(&dir, "MumbleWay").unwrap();
        assert!(second.check().is_ok(), "the replacement must be usable");
        assert_ne!(second.cert_pem, first.cert_pem);

        let kept = files_starting(&dir, "identity.key.broken-");
        assert_eq!(kept.len(), 1, "the broken key is kept, not deleted");
        assert_eq!(std::fs::read_to_string(dir.join(&kept[0])).unwrap(), half);
        assert_eq!(files_starting(&dir, "identity.crt.broken-").len(), 1);

        // And the new pair is what loads from now on.
        let third = Identity::load_or_create(&dir, "MumbleWay").unwrap();
        assert_eq!(third.cert_pem, second.cert_pem);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_lone_half_is_never_overwritten() {
        let dir = scratch_dir("lone");
        let first = Identity::load_or_create(&dir, "MumbleWay").unwrap();
        std::fs::remove_file(dir.join("identity.crt")).unwrap();

        let second = Identity::load_or_create(&dir, "MumbleWay").unwrap();
        assert_ne!(second.key_pem, first.key_pem);
        let kept = files_starting(&dir, "identity.key.broken-");
        assert_eq!(kept.len(), 1);
        assert_eq!(
            std::fs::read_to_string(dir.join(&kept[0])).unwrap(),
            first.key_pem,
            "the surviving half of the old identity must still exist"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_key_from_another_identity_is_not_trusted() {
        let dir = scratch_dir("mismatch");
        let first = Identity::load_or_create(&dir, "MumbleWay").unwrap();
        let other = Identity::generate("other").unwrap();
        std::fs::write(dir.join("identity.key"), &other.key_pem).unwrap();
        assert!(Identity {
            cert_pem: first.cert_pem.clone(),
            key_pem: other.key_pem.clone(),
        }
        .check()
        .is_err());

        let second = Identity::load_or_create(&dir, "MumbleWay").unwrap();
        assert!(second.check().is_ok());
        assert_ne!(second.cert_pem, first.cert_pem);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn writing_leaves_no_temporary_files() {
        let dir = scratch_dir("tmp");
        Identity::load_or_create(&dir, "MumbleWay").unwrap();
        assert_eq!(
            files_starting(&dir, "identity"),
            vec!["identity.crt".to_string(), "identity.key".to_string()]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn the_key_is_readable_by_its_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch_dir("mode");
        Identity::load_or_create(&dir, "MumbleWay").unwrap();
        let key = dir.join("identity.key");
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&key), 0o600);

        // A key left 0644 by an older build is tightened, not replaced.
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o644)).unwrap();
        let before = std::fs::read_to_string(&key).unwrap();
        Identity::load_or_create(&dir, "MumbleWay").unwrap();
        assert_eq!(mode(&key), 0o600);
        assert_eq!(std::fs::read_to_string(&key).unwrap(), before);
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
