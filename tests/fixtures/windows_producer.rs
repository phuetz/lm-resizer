use std::io::{self, Write};

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("utf16") => {
            let mut bytes = vec![0xff, 0xfe];
            for unit in "OUT début\r\nERROR erreur utile été résumé\r\n".encode_utf16() {
                bytes.extend_from_slice(&unit.to_le_bytes());
            }
            io::stdout().write_all(&bytes).unwrap();
        }
        Some("dual") => {
            io::stdout().write_all(b"OUT begin\r\n").unwrap();
            io::stdout().flush().unwrap();
            io::stderr().write_all("ERROR été\r\n".as_bytes()).unwrap();
            io::stderr().flush().unwrap();
            io::stdout().write_all(b"OUT end\0\r\n").unwrap();
            io::stdout().flush().unwrap();
        }
        _ => panic!("unknown fixture"),
    }
    std::process::exit(23);
}
