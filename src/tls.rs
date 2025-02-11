use std::io::{BufReader, Cursor};

pub fn build_cert(c: &str) -> std::io::Result<Vec<Vec<u8>>> {
    let c = Cursor::new(c);
    todo!("not impl")
}
/*
fn client_config() -> Result<ClientConfig, std::io::Error> {

}

fn server_config() -> Result<ServerConfig, std::io::Error> {

}
 */
