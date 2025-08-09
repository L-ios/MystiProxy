use std::{thread::sleep, time};

fn main() {
    loop {
        sleep(time::Duration::from_secs(10));
        println!("hello")
    }
}