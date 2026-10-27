# TLSNotary x.com spike

Checks whether a TLSNotary prover (MPC-TLS, tlsn `eb19c82`) can notarize
responses from x.com hosts, and measures time and prover upload per proof.

`x_spike.rs` runs prover and verifier in one process and counts the bytes
between them; the prover connects to the real server. The GitHub workflow
`.github/workflows/tlsnotary-x-spike.yml` builds it inside a tlsn checkout and
runs three proofs: `x.com/robots.txt`, the `api.x.com` guest-token endpoint,
and a public post from the syndication endpoint.
