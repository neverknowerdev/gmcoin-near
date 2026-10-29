//! OutLayer module: scores one batch of users' GM posts for one day.
//! stdin: `{"since","until","users":[[uid,x_id]],"scoring"}`; stdout: `{"ok","scores":[[uid,points]]}`.

use x_client::{scan, CachedXApi, ScanInput, ScanOutput};

fn main() {
    let out = match (
        serde_json::from_str::<ScanInput>(&wasi_http::read_stdin()),
        wasi_http::config(),
    ) {
        (Ok(input), Ok(cfg)) => scan(
            &CachedXApi {
                http: &wasi_http::WasiHttp,
                cfg: &cfg,
            },
            &input,
        ),
        (Err(e), _) => ScanOutput {
            error: Some(format!("bad input: {e}")),
            ..Default::default()
        },
        (_, Err(e)) => ScanOutput {
            error: Some(e),
            ..Default::default()
        },
    };
    wasi_http::write_stdout(&serde_json::to_string(&out).unwrap());
}
