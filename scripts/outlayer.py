#!/usr/bin/env python3
"""OutLayer helpers without the outlayer CLI.

  upload  <file> [--path P]   store a file on FastFS (a __fastdata_fastfs tx to the
                              OutLayer contract); prints its URL and sha256
  secrets <wasm> --profile P --json '{"X_API_KEY":"..."}'
                              encrypt secrets for the module's WASM hash and store them
  estimate                    print estimate_execution_cost for the deploy config limits

Signs with near-cli-rs and the legacy keychain, as --account.
FastFS URL: https://<test|main>.fastfs.io/<account>/<outlayer contract>/<path>.
"""

import argparse
import base64
import hashlib
import json
import os
import pathlib
import struct
import subprocess
import sys
import time
import urllib.request

NETS = {
    "testnet": {"contract": "outlayer.testnet", "fastfs": "test.fastfs.io", "api": "https://testnet-api.outlayer.ai"},
    "mainnet": {"contract": "outlayer.near", "fastfs": "main.fastfs.io", "api": "https://api.outlayer.ai"},
}


def near(*args):
    r = subprocess.run(["near", *args], capture_output=True, text=True)
    return r.stdout + r.stderr


def borsh_str(s):
    b = s.encode()
    return struct.pack("<I", len(b)) + b


def borsh_bytes(b):
    return struct.pack("<I", len(b)) + b


def fastfs_payload(path, mime, content):
    # enum FastfsData { Simple(SimpleFastfs) } -> variant 0;
    # SimpleFastfs { relative_path, content: Some(FastfsFileContent { mime_type, content }) }
    return b"\x00" + borsh_str(path) + b"\x01" + borsh_str(mime) + borsh_bytes(content)


def upload(a, net):
    data = pathlib.Path(a.file).read_bytes()
    sha = hashlib.sha256(data).hexdigest()
    path = a.path or f"{sha}.wasm"
    mime = a.mime or ("application/wasm" if path.endswith(".wasm") else "application/json")
    args = base64.b64encode(fastfs_payload(path, mime, data)).decode()
    # No contract implements the method: the receipt fails by design and the
    # FastFS indexer serves the file from the transaction data.
    near("contract", "call-function", "as-transaction", net["contract"], "__fastdata_fastfs",
         "base64-args", args, "prepaid-gas", "300.0 Tgas", "attached-deposit", "0 NEAR",
         "sign-as", a.account, "network-config", a.network, "sign-with-legacy-keychain", "send")
    url = f"https://{net['fastfs']}/{a.account}/{net['contract']}/{path}"
    for _ in range(30):
        try:
            with urllib.request.urlopen(url, timeout=10) as r:
                body = r.read()
                if hashlib.sha256(body).hexdigest() == sha:
                    print(json.dumps({"url": url, "hash": sha}))
                    return
        except Exception:
            pass
        time.sleep(3)
    sys.exit(f"uploaded but {url} does not serve the file yet")


def secrets(a, net):
    from cryptography.hazmat.primitives import hashes
    from cryptography.hazmat.primitives.asymmetric.x25519 import X25519PrivateKey, X25519PublicKey
    from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305
    from cryptography.hazmat.primitives.kdf.hkdf import HKDF
    from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat

    sha = hashlib.sha256(pathlib.Path(a.wasm).read_bytes()).hexdigest()
    plain = json.dumps(json.loads(a.json)).encode()
    req = urllib.request.Request(
        f"{net['api']}/secrets/pubkey",
        data=json.dumps({"accessor": {"type": "WasmHash", "hash": sha}, "owner": a.account,
                         "profile": a.profile, "secrets_json": plain.decode()}).encode(),
        headers={"content-type": "application/json", "user-agent": "gmcoin-near-deploy/0.1"},
    )
    pubkey = json.loads(urllib.request.urlopen(req, timeout=20).read())["pubkey"]
    # ECIES v1: X25519 -> HKDF-SHA256("outlayer-keystore-v1") -> ChaCha20-Poly1305
    eph = X25519PrivateKey.generate()
    key = HKDF(algorithm=hashes.SHA256(), length=32, salt=None, info=b"outlayer-keystore-v1").derive(
        eph.exchange(X25519PublicKey.from_public_bytes(bytes.fromhex(pubkey))))
    nonce = os.urandom(12)
    blob = bytes([1]) + eph.public_key().public_bytes(Encoding.Raw, PublicFormat.Raw) + nonce + \
        ChaCha20Poly1305(key).encrypt(nonce, plain, None)
    out = near("contract", "call-function", "as-transaction", net["contract"], "store_secrets", "json-args",
               json.dumps({"accessor": {"WasmHash": {"hash": sha}}, "profile": a.profile,
                           "encrypted_secrets_base64": base64.b64encode(blob).decode(),
                           "access": "AllowAll", "vault_id": None}),
               "prepaid-gas", "100.0 Tgas", "attached-deposit", "0.1 NEAR",
               "sign-as", a.account, "network-config", a.network, "sign-with-legacy-keychain", "send")
    if "Error" in out or "panicked" in out:
        sys.exit(out)
    print(json.dumps({"hash": sha, "profile": a.profile, "owner": a.account}))


def estimate(a, net):
    c = json.loads((pathlib.Path(__file__).resolve().parent.parent / "deploy" / f"{a.network}.json").read_text())["modules"]
    limits = {"max_instructions": int(c["max_instructions"]), "max_memory_mb": c["max_memory_mb"],
              "max_execution_seconds": c["max_execution_seconds"]}
    print(near("contract", "call-function", "as-read-only", net["contract"], "estimate_execution_cost",
               "json-args", json.dumps({"resource_limits": limits, "input_len": 2000}),
               "network-config", a.network, "now"))


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--network", default="testnet")
    p.add_argument("--account", help="signer (the FastFS uploader / secrets owner)")
    sub = p.add_subparsers(dest="cmd", required=True)
    u = sub.add_parser("upload")
    u.add_argument("file")
    u.add_argument("--path")
    u.add_argument("--mime")
    s = sub.add_parser("secrets")
    s.add_argument("wasm")
    s.add_argument("--profile", required=True)
    s.add_argument("--json", required=True)
    sub.add_parser("estimate")
    a = p.parse_args()
    {"upload": upload, "secrets": secrets, "estimate": estimate}[a.cmd](a, NETS[a.network])


if __name__ == "__main__":
    main()
