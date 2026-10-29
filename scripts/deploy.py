#!/usr/bin/env python3
"""Deploys GM token + core to a network with near-cli-rs.

    scripts/deploy.py --network testnet --master gmnear-dev.testnet [--staging] [--mock-outlayer]

Creates <prefix>token, core, treasury (and outlayer-mock) subaccounts of the
master account, deploys the WASM from res/ (run scripts/build.sh first),
initialises both contracts from deploy/<network>.json and writes the
resulting account ids to deploy/<network>.<master>.out.json.

Keys of new accounts go to the legacy keychain (~/.near-credentials).
Mainnet deploys are run by hand; CI only ever holds testnet keys.
"""

import argparse
import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
GM = 10**18
NEAR = 10**24
NS = 10**9


def near(*args, check=True):
    cmd = ["near", *args]
    print("+", " ".join(a if len(a) < 120 else a[:117] + "..." for a in cmd), flush=True)
    r = subprocess.run(cmd, capture_output=True, text=True)
    out = r.stdout + r.stderr
    if check and (r.returncode != 0 or "Error" in out and "successfully" not in out):
        print(out, file=sys.stderr)
        sys.exit(f"command failed: {' '.join(cmd[:4])}")
    return out


def to_yocto(near_str):
    whole, _, frac = str(near_str).partition(".")
    return int(whole) * NEAR + int((frac + "0" * 24)[:24] or 0)


def account_exists(acc, network):
    out = near("account", "view-account-summary", acc, "network-config", network, "now", check=False)
    return "Native account balance" in out


def create_sub(acc, master, network, balance_near):
    if account_exists(acc, network):
        print(f"  {acc} exists, reusing")
        return
    near("account", "create-account", "fund-myself", acc, f"{balance_near} NEAR",
         "autogenerate-new-keypair", "save-to-legacy-keychain", "sign-as", master,
         "network-config", network, "sign-with-legacy-keychain", "send")


def call(contract, method, args, signer, network, gas="300.0 Tgas", deposit="0 NEAR"):
    return near("contract", "call-function", "as-transaction", contract, method,
                "json-args", json.dumps(args), "prepaid-gas", gas, "attached-deposit", deposit,
                "sign-as", signer, "network-config", network, "sign-with-legacy-keychain", "send")


def deploy(acc, wasm, network, init_args=None, init_method="new"):
    args = ["contract", "deploy", acc, "use-file", str(ROOT / "res" / wasm)]
    if init_args is None:
        args += ["without-init-call"]
    else:
        args += ["with-init-call", init_method, "json-args", json.dumps(init_args),
                 "prepaid-gas", "100.0 Tgas", "attached-deposit", "0 NEAR"]
    args += ["network-config", network, "sign-with-legacy-keychain", "send"]
    near(*args)


def core_config(c):
    e = c["core"]["emission"]
    k = c["core"]
    return {
        "emission": {
            "b0": str(int(e["b0_gm"]) * GM),
            "decay_bps": e["decay_bps"],
            "step_cap_bps": e["step_cap_bps"],
            "floor_bps": e["floor_bps"],
            "ceiling_mult": e["ceiling_mult"],
            "baseline_active": e["baseline_active"],
            "max_supply": str(int(c["token"]["max_supply_gm"]) * GM),
            "treasury_bps": e["treasury_bps"],
            "active_min_days": e["active_min_days"],
        },
        "unverified_cap": str(int(k["unverified_cap_gm"]) * GM),
        "tip_amount": str(int(k["tip_gm"]) * GM),
        "tip_daily_limit": str(int(k["tip_daily_limit_gm"]) * GM),
        "register_deposit": str(to_yocto(k["register_deposit_near"])),
        "sponsored_storage_cost": str(to_yocto(k["sponsored_storage_cost_near"])),
        "sponsor_checks_per_user_per_day": k["sponsor_checks_per_user_per_day"],
        "sponsor_daily_budget": str(to_yocto(k["sponsor_daily_budget_near"])),
        "sponsor_registrations_per_day": k["sponsor_registrations_per_day"],
        "sponsor_reclaim_after_ns": str(k["sponsor_reclaim_after_days"] * c["day_length_seconds"] * NS),
        "scan_delay_ns": str(k["scan_delay_seconds"] * NS),
        "scan_batch_size": k["scan_batch_size"],
        "scan_max_in_flight": k["scan_max_in_flight"],
        "scan_max_attempts": k["scan_max_attempts"],
        "settle_chunk": k["settle_chunk"],
        "scoring": {
            "points_simple": 1, "points_hashtag": 3, "points_cashtag": 5, "points_per_like": 1,
            "max_hashtag_posts": 10, "max_cashtag_posts": 10, "max_simple_posts": 4294967295,
            "max_likes_per_post": 100, "spam_post_limit": 1000,
        },
    }


def module_ref(c, url, sha):
    m = c["modules"]
    return {
        "source": {"WasmUrl": {"url": url, "hash": sha, "build_target": "wasm32-wasip2"}},
        "max_instructions": m["max_instructions"],
        "max_memory_mb": m["max_memory_mb"],
        "max_execution_seconds": str(m["max_execution_seconds"]),
        "deposit": str(to_yocto(m["deposit_near"])),
    }


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--network", default="testnet")
    p.add_argument("--master", required=True, help="owner account; contracts are its subaccounts")
    p.add_argument("--prefix", default="", help="subaccount prefix, e.g. pr-42- for PR previews")
    p.add_argument("--staging", action="store_true", help="deploy core with staging methods (advance_clock)")
    p.add_argument("--mock-outlayer", action="store_true", help="deploy the mock OutLayer and point core at it")
    p.add_argument("--outlayer", help="OutLayer contract id (default outlayer.<network>)")
    p.add_argument("--modules", help="JSON file with x_verify/x_scan {url, hash} to set after deploy")
    p.add_argument("--secrets-profile", help="OutLayer secrets profile owned by --master")
    a = p.parse_args()

    if a.network == "mainnet" and a.staging:
        sys.exit("refusing to deploy a staging build to mainnet")
    c = json.loads((ROOT / "deploy" / f"{a.network}.json").read_text())
    sub = lambda name: f"{a.prefix}{name}.{a.master}"
    token, core, treasury = sub("token"), sub("core"), sub("treasury")
    outlayer = a.outlayer or ("outlayer.near" if a.network == "mainnet" else "outlayer.testnet")

    create_sub(treasury, a.master, a.network, 0.1)
    create_sub(token, a.master, a.network, 3)
    create_sub(core, a.master, a.network, 8)
    if a.mock_outlayer:
        outlayer = sub("outlayer-mock")
        create_sub(outlayer, a.master, a.network, 2)
        deploy(outlayer, "mock_outlayer.wasm", a.network, {})

    deploy(token, "gm_token.wasm", a.network, {
        "owner": a.master,
        "minter": core,
        "treasury": treasury,
        "metadata": {"spec": "ft-1.0.0", "name": "GM", "symbol": "GM", "decimals": 18},
        "max_supply": str(int(c["token"]["max_supply_gm"]) * GM),
        "daily_mint_cap": str(int(c["token"]["daily_mint_cap_gm"]) * GM),
        "fee_bps": c["token"]["fee_bps"],
    })
    deploy(core, "gm_core_staging.wasm" if a.staging else "gm_core.wasm", a.network, {
        "owner": a.master,
        "token": token,
        "treasury": treasury,
        "outlayer": outlayer,
        "config": core_config(c),
        "day_length_ns": str(c["day_length_seconds"] * NS),
        "days_per_epoch": c["days_per_epoch"],
    })
    # Core and treasury never pay a transfer fee.
    call(token, "set_fee_exempt", {"account_id": core, "exempt": True}, a.master, a.network)

    if a.modules or a.mock_outlayer:
        mods = json.loads(pathlib.Path(a.modules).read_text()) if a.modules else {
            "x_verify": {"url": "https://test.fastfs.io/mock/x-verify.wasm", "hash": "00" * 32},
            "x_scan": {"url": "https://test.fastfs.io/mock/x-scan.wasm", "hash": "00" * 32},
        }
        call(core, "set_modules", {"modules": {
            "x_verify": module_ref(c, mods["x_verify"]["url"], mods["x_verify"]["hash"]),
            "x_scan": module_ref(c, mods["x_scan"]["url"], mods["x_scan"]["hash"]),
            "secrets": {"profile": a.secrets_profile, "account_id": a.master} if a.secrets_profile else None,
        }}, a.master, a.network)

    out = {"network": a.network, "master": a.master, "token": token, "core": core,
           "treasury": treasury, "outlayer": outlayer, "staging": a.staging}
    path = ROOT / "deploy" / f"{a.network}.{a.prefix}{a.master}.out.json"
    path.write_text(json.dumps(out, indent=2) + "\n")
    print(json.dumps(out, indent=2))


if __name__ == "__main__":
    main()
