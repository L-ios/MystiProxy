use rustls_pemfile::Item;
use std::io::{BufReader, Cursor, Error};
use tokio_rustls::rustls::pki_types::PrivateKeyDer;

pub fn build_cert(c: &str) -> std::io::Result<Vec<Vec<u8>>> {
    let c = Cursor::new(c);
    todo!("not impl")
}

fn client_config() {

}
/*
fn server_config() -> Result<ServerConfig, std::io::Error> {

}
 */

/*
pub fn load_certs(c_content: &[u8]) -> std::io::Result<Vec<Vec<u8>>>{
    let cur = Cursor::new(c_content);
    let mut reader = BufReader::new(cur);
    Ok(rustls_pemfile::certs(&mut reader)
        .into_iter()
        .map(|v| v?.iter().as_slice().iter().collect())
        .collect::<Vec<_>>())
}

pub fn read_key<'a>(key_content: Vec<u8>) -> std::io::Result<PrivateKeyDer<'a>> {
    let cur = Cursor::new(key_content);
    let mut reader = BufReader::new(cur);
    match rustls_pemfile::read_one(&mut reader)? {
        None => Err(Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid key",
        )),
        Some(key) => Ok(
            match tokio_rustls::rustls::pki_types::PrivateKeyDer::try_from(match key {
                Item::X509Certificate(cate) => cate.iter().as_slice(),
                Item::SubjectPublicKeyInfo(info) => info.iter().as_ref(),
                Item::Pkcs1Key(key) => key.secret_pkcs1_der(),
                Item::Pkcs8Key(key) => key.secret_pkcs8_der(),
                Item::Sec1Key(key) => key.secret_sec1_der(),
                Item::Crl(crl) => crl.iter().as_slice(),
                Item::Csr(csr) => csr.iter().as_slice(),
                _ => return Err(Error::new(std::io::ErrorKind::InvalidData, "invalid key"))
            }) {
                Ok(value) => value,
                Err(err) => return Err(Error::new(std::io::ErrorKind::InvalidData, err)),
            }
        )
    }

}

 */