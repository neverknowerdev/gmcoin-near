import { KeyPair, KeyPairSigner, actions, buildDelegateAction, encodeSignedDelegate, base64Encode, type Action } from "near-api-js";

export async function makeDelegate(acts: Action[], o: { receiverId?: string; maxBlockHeight?: bigint } = {}): Promise<string> {
  const kp = KeyPair.fromRandom("ed25519");
  const signer = new KeyPairSigner(kp);
  const da = buildDelegateAction({
    senderId: "alice.testnet", receiverId: o.receiverId ?? "core.gmcoin.testnet", actions: acts,
    nonce: 1n, maxBlockHeight: o.maxBlockHeight ?? 1500n, publicKey: kp.getPublicKey(),
  });
  const { signedDelegate: sd } = await signer.signDelegateAction(da);
  return base64Encode(encodeSignedDelegate(sd));
}
export const fc = (m: string, deposit = 0n, gas = 30_000_000_000_000n) =>
  actions.functionCall(m, new Uint8Array(), gas, deposit);

export class MemKV {
  m = new Map<string, string>();
  async get(k: string) { return this.m.get(k) ?? null; }
  async put(k: string, v: string) { this.m.set(k, v); }
}
