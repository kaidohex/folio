use folio::feed::fetch_and_parse_debug;
fn main() {
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "https://hnrss.org/frontpage".into());
    let client = reqwest::blocking::Client::builder()
        .user_agent("Mozilla/5.0 (compatible; Folio/0.1; RSS reader)")
        .timeout(std::time::Duration::from_secs(25))
        .build()
        .unwrap();
    match fetch_and_parse_debug(&client, &url) {
        Ok(f) => println!(
            "OK: {:?} entries={}",
            f.title.map(|t| t.content),
            f.entries.len()
        ),
        Err(e) => println!("ERR: {e}"),
    }
}
