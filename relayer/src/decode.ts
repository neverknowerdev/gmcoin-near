import { deserialize } from "borsh";
import { SCHEMA, base64Decode, type SignedDelegate } from "near-api-js";
import type { DecodedDelegate } from "./policy";

export interface Decoded { signed: SignedDelegate; delegate: DecodedDelegate }

/** base64(borsh SignedDelegate) -> raw library object + policy-friendly view. Throws on malformed input. */
export function decodeSignedDelegate(b64: string): Decoded {
  const signed = deserialize(SCHEMA.SignedDelegate, base64Decode(b64)) as SignedDelegate;
  const da = signed.delegateAction;
  return {
    signed,
    delegate: {
      senderId: da.senderId, receiverId: da.receiverId, maxBlockHeight: BigInt(da.maxBlockHeight),
      // borsh yields enum variants as plain objects with a single key, e.g. { functionCall: {...} }
      actions: da.actions.map((a) => {
        const type = Object.keys(a)[0];
        const body = (a as any)[type] ?? {};
        return { type, methodName: body.methodName as string | undefined, deposit: BigInt(body.deposit ?? 0n), gas: BigInt(body.gas ?? 0n) };
      }),
    },
  };
}
