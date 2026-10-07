#[test]
fn success() { assert_eq!(2 + 2, 4); }
#[test]
fn invoice_total() { assert_eq!(12030, 12031); }
#[test]
fn config_path() { assert_eq!("~/.config/app", "~/.config/other"); }
