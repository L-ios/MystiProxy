use std::io::Cursor;

pub fn build_cert(c: &str) -> std::io::Result<Vec<Vec<u8>>> {
    let c = Cursor::new(c);
    todo!("not impl")
}
