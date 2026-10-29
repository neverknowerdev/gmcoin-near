/** Plain fetch JSON-RPC view calls. */

export async function callView<T = unknown>(
  rpc: string,
  account: string,
  method: string,
  args: Record<string, unknown> = {},
  fetchImpl: typeof fetch = fetch,
): Promise<T> {
  const body = {
    jsonrpc: "2.0",
    id: "gm",
    method: "query",
    params: {
      request_type: "call_function",
      finality: "optimistic",
      account_id: account,
      method_name: method,
      args_base64: btoa(JSON.stringify(args)),
    },
  };
  const res = await fetchImpl(rpc, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`RPC HTTP ${res.status}`);
  return decodeViewResponse<T>(await res.json(), method);
}

export function decodeViewResponse<T = unknown>(json: any, method = "view"): T {
  if (json?.error) {
    const msg = json.error?.cause?.info?.error_message ?? json.error?.data ?? json.error?.message ?? "RPC error";
    throw new Error(`${method}: ${typeof msg === "string" ? msg : JSON.stringify(msg)}`);
  }
  const result = json?.result;
  if (result?.error) throw new Error(`${method}: ${String(result.error)}`);
  const bytes: unknown = result?.result;
  if (!Array.isArray(bytes)) throw new Error(`${method}: empty RPC result`);
  const text = new TextDecoder().decode(new Uint8Array(bytes as number[]));
  if (text === "") return null as T;
  return JSON.parse(text) as T;
}
