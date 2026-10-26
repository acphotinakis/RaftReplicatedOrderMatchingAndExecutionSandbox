//! HTTP-to-dataset regression coverage without external credentials or services.

use std::{fs, io::{Read, Write}, net::TcpListener, sync::atomic::{AtomicU64, Ordering}, thread, time::Duration};
use raft_replicated_order_matching_and_execution_sandbox::market::{
    alpaca::{AlpacaClient, AlpacaConfig, Credentials},
    download::{DownloadConfig, HistoricalTradeDownloader},
};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[test]
fn paginated_download_resumes_without_duplicate_records() {
    // Given two HTTP pages in a single exact time window.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        for page in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            let request = String::from_utf8(request).unwrap();
            assert!(request.starts_with("GET /v2/stocks/trades?"));
            assert!(request.to_ascii_lowercase().contains("apca-api-key-id: fixture-key"));
            assert_eq!(request.contains("page_token=second"), page == 1);
            let token = if page == 0 { "\"second\"" } else { "null" };
            let body = format!(r#"{{"trades":{{"SPY":[{{"t":"2024-01-02T15:00:0{page}Z","p":100.25,"s":10,"x":"V","i":{page},"c":[],"z":"C"}}]}},"next_page_token":{token}}}"#);
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
        }
    });
    let root = std::env::temp_dir().join(format!("market-download-{}-{}", std::process::id(), SEQUENCE.fetch_add(1, Ordering::Relaxed)));
    let client = AlpacaClient::new(Credentials::new("fixture-key", "fixture-secret").unwrap(), AlpacaConfig {
        base_url: format!("http://{address}"), timeout: Duration::from_secs(2), max_attempts: 1,
        requests_per_minute: u32::MAX,
    }).unwrap();
    let downloader = HistoricalTradeDownloader::new(client, DownloadConfig {
        output_dir: root.clone(), parquet_output_dir: None, feed: "sip".into(), limit: 1,
    }).unwrap();
    let start = "2024-01-02T15:00:00Z".parse().unwrap();
    let end = "2024-01-02T15:01:00Z".parse().unwrap();

    // When the same download is executed twice, the second run uses its checkpoint.
    assert_eq!(downloader.download_symbol("SPY", start, end).unwrap(), 2);
    server.join().unwrap();
    assert_eq!(downloader.download_symbol("SPY", start, end).unwrap(), 0);

    // Then only the two original records are present, in page order.
    let path = root.join("SPY_historical_trades_sip.csv");
    let records = csv::Reader::from_path(path).unwrap().records().collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].get(5), Some("0"));
    assert_eq!(records[1].get(5), Some("1"));
    fs::remove_dir_all(root).unwrap();
}
