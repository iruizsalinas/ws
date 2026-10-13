//! TLS configuration (rustls with the ring provider; TLS 1.2 and 1.3 only).

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ClientConfig, RootCertStore, ServerConfig};
use std::sync::{Arc, OnceLock};

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// PEM text itself, or the file it names. Anything with a PEM header or a
/// line break is text, never a path, so errors name the file or just say
/// "PEM text": never the text, which may hold a private key.
struct Pem {
    data: Vec<u8>,
    name: String,
}

impl Pem {
    fn load(source: &str) -> Result<Pem, String> {
        if source.contains("-----BEGIN") || source.contains(['\n', '\r']) {
            return Ok(Pem { data: source.as_bytes().to_vec(), name: "PEM text".into() });
        }
        let data = std::fs::read(source).map_err(|e| format!("cannot read {source}: {e}"))?;
        Ok(Pem { data, name: source.to_string() })
    }

    fn certs(&self) -> Result<Vec<CertificateDer<'static>>, String> {
        let certs = CertificateDer::pem_slice_iter(&self.data)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("invalid certificates in {}: {e}", self.name))?;
        if certs.is_empty() {
            return Err(format!("no certificates in {}", self.name));
        }
        Ok(certs.into_iter().map(CertificateDer::into_owned).collect())
    }

    fn key(&self) -> Result<PrivateKeyDer<'static>, String> {
        PrivateKeyDer::from_pem_slice(&self.data).map_err(|e| format!("invalid private key in {}: {e}", self.name))
    }
}

/// Client config trusting `ca` (PEM text or a PEM file) or else Mozilla's roots.
pub fn client(ca: Option<&str>) -> Result<Arc<ClientConfig>, String> {
    static DEFAULT: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    let build = |roots: RootCertStore| -> Result<Arc<ClientConfig>, String> {
        let config = ClientConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .with_root_certificates(roots)
            .with_no_client_auth();
        Ok(Arc::new(config))
    };
    match ca {
        Some(ca) => {
            let pem = Pem::load(ca)?;
            let mut roots = RootCertStore::empty();
            for cert in pem.certs()? {
                roots.add(cert).map_err(|e| format!("invalid CA certificate in {}: {e}", pem.name))?;
            }
            build(roots)
        }
        None => Ok(DEFAULT
            .get_or_init(|| {
                let roots = RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
                build(roots).expect("ring supports the default protocol versions")
            })
            .clone()),
    }
}

/// Server config from a certificate chain and its private key, each PEM
/// text or a PEM file.
pub fn server(cert: &str, key: &str) -> Result<Arc<ServerConfig>, String> {
    let chain = Pem::load(cert)?.certs()?;
    let key = Pem::load(key)?.key()?;
    let config = ServerConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_no_client_auth()
        .with_single_cert(chain, key)
        .map_err(|e| format!("invalid certificate or key: {e}"))?;
    Ok(Arc::new(config))
}
