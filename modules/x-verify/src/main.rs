//! OutLayer module: finds a GM link code on X and returns its author.
//! stdin: `{"code","account_id","post_id"?}`; stdout: `{"ok","x_id","handle","post_id","error"}`.

use x_client::{verify, CachedXApi, VerifyInput, VerifyOutput};

fn main() {
    let out = match (
        serde_json::from_str::<VerifyInput>(&wasi_http::read_stdin()),
        wasi_http::config(),
    ) {
        (Ok(input), Ok(cfg)) => verify(
            &CachedXApi {
                http: &wasi_http::WasiHttp,
                cfg: &cfg,
            },
            &input,
        ),
        (Err(e), _) => VerifyOutput {
            error: Some(format!("bad input: {e}")),
            ..Default::default()
        },
        (_, Err(e)) => VerifyOutput {
            error: Some(e),
            ..Default::default()
        },
    };
    wasi_http::write_stdout(&serde_json::to_string(&out).unwrap());
}
