//! Glue shared by the OutLayer modules: HTTP over wasi-http, JSON input on
//! stdin, JSON output on stdout, secrets from env vars.

use std::io::{Read, Write};
use std::time::Duration;

use x_client::{HttpClient, XApiConfig};

pub struct WasiHttp;

impl HttpClient for WasiHttp {
    fn get(&self, url: &str, headers: &[(String, String)]) -> Result<(u16, Vec<u8>), String> {
        let mut req = wasi_http_client::Client::new()
            .get(url)
            .connect_timeout(Duration::from_secs(10));
        for (k, v) in headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let resp = req.send().map_err(|e| format!("http: {e}"))?;
        let status = resp.status();
        let body = resp.body().map_err(|e| format!("http body: {e}"))?;
        Ok((status, body))
    }
}

pub fn read_stdin() -> String {
    let mut s = String::new();
    let _ = std::io::stdin().read_to_string(&mut s);
    s
}

pub fn write_stdout(json: &str) {
    let mut out = std::io::stdout();
    let _ = out.write_all(json.as_bytes());
    let _ = out.flush();
}

pub fn config() -> Result<XApiConfig, String> {
    XApiConfig::from_env(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
}
