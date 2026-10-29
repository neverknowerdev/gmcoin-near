export function isRateLimit(e: unknown): boolean {
  const msg = e instanceof Error ? `${e.name} ${e.message}` : String(e);
  return /rate.?limit|too many requests|\b429\b/i.test(msg);
}

/** Retries `fn` with exponential backoff when it fails with a rate-limit error. */
export async function retryRateLimit<T>(
  fn: () => Promise<T>,
  opts: { tries?: number; baseMs?: number; sleep?: (ms: number) => Promise<void> } = {},
): Promise<T> {
  const tries = opts.tries ?? 6;
  const base = opts.baseMs ?? 2000;
  const sleep = opts.sleep ?? ((ms: number) => new Promise<void>((r) => setTimeout(r, ms)));
  for (let i = 0; ; i++) {
    try {
      return await fn();
    } catch (e) {
      if (i + 1 >= tries || !isRateLimit(e)) throw e;
      await sleep(base * 2 ** i);
    }
  }
}
