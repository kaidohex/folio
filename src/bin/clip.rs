fn main() {
    match arboard::Clipboard::new().and_then(|mut cb| cb.set_text("folio-test-link".to_string())) {
        Ok(_) => println!("CLIPBOARD OK"),
        Err(e) => println!("CLIPBOARD ERR: {e}"),
    }
}
