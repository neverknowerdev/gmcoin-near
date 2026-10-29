export const POST_TEXT_PREFIX = "gm ";

/** Text to post on X: the link code, so x-verify can find it by search. */
export function linkPostText(code: string): string {
  return `${POST_TEXT_PREFIX}${code}`;
}

export function intentUrl(code: string): string {
  return `https://x.com/intent/post?text=${encodeURIComponent(linkPostText(code))}`;
}

export function profileUrl(handle: string): string {
  return `https://x.com/${encodeURIComponent(handle.replace(/^@/, ""))}`;
}
