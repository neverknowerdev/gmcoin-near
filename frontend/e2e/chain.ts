import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { Account } from "@near-js/accounts";
import { KeyPair } from "@near-js/crypto";
import { KeyPairSigner } from "@near-js/signers";
import { JsonRpcProvider } from "@near-js/providers";
import { retryRateLimit } from "../src/lib/retry";
import { callView } from "../src/lib/rpc";
import { parseStatus } from "../src/lib/views";

export const RPC = process.env.VITE_RPC ?? "https://rpc.testnet.fastnear.com";
export const CORE = "mock-core.gmnear-dev-10272324.testnet";
export const MOCK_OUTLAYER = "mock-outlayer-mock.gmnear-dev-10272324.testnet";

const provider = () => new JsonRpcProvider({ url: RPC }, { retries: 5, wait: 2000, backoff: 2 });

export function loadAccount(id: string): Account {
  const file = join(homedir(), ".near-credentials", "testnet", `${id}.json`);
  const key = JSON.parse(readFileSync(file, "utf8")).private_key as string;
  return new Account(id, provider(), KeyPairSigner.fromSecretKey(key as `ed25519:${string}`));
}

export async function createFundedSubaccount(master: Account, prefix: string, near: bigint) {
  const kp = KeyPair.fromRandom("ed25519");
  const id = `${prefix}.${master.accountId}`;
  await retryRateLimit(() => master.createAccount(id, kp.getPublicKey(), near));
  return { id, privateKey: kp.toString() };
}

export async function call(acc: Account, contractId: string, methodName: string, args: object) {
  return retryRateLimit(() => acc.callFunction({ contractId, methodName, args }));
}

export const coreStatus = () => callView(RPC, CORE, "status").then(parseStatus);
