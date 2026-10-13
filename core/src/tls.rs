//! TLS configuration (rustls with the ring provider; TLS 1.2 and 1.3 only).

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ClientConfig, RootCertStore, ServerConfig};
use std::sync::{Arc, OnceLock};

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn certs(path: &str) -> Result<Vec<CertificateDer<'static>>, String> {
    let certs = CertificateDer::pem_file_iter(path)
        .and_then(|it| it.collect::<Result<Vec<_>, _>>())
        .map_err(|e| format!("cannot read certificates from {path}: {e}"))?;
    if certs.is_empty() {
        return Err(format!("no certificates in {path}"));
    }
    Ok(certs)
}

/// Client config trusting `ca` (a PEM file) or else Mozilla's root store.
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
        Some(path) => {
            let mut roots = RootCertStore::empty();
            for cert in certs(path)? {
                roots.add(cert).map_err(|e| format!("invalid CA certificate in {path}: {e}"))?;
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

/// Server config from PEM certificate chain and private key files.
pub fn server(cert: &str, key: &str) -> Result<Arc<ServerConfig>, String> {
    let chain = certs(cert)?;
    let key = PrivateKeyDer::from_pem_file(key).map_err(|e| format!("cannot read private key from {key}: {e}"))?;
    let config = ServerConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_no_client_auth()
        .with_single_cert(chain, key)
        .map_err(|e| format!("invalid certificate or key: {e}"))?;
    Ok(Arc::new(config))
}
