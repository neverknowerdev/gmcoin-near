// Wallet selector and near-js expect Node's Buffer in the browser.
import { Buffer } from "buffer";

const g = globalThis as unknown as { Buffer?: typeof Buffer; global?: unknown };
if (!g.Buffer) g.Buffer = Buffer;
if (!g.global) g.global = globalThis;
