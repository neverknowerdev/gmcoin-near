export type Network = "testnet" | "mainnet";

export interface AppConfig {
  network: Network;
  core: string;
  token: string;
  rpc: string;
  relayer: string | null;
  staging: boolean;
  /** problems that make the app unusable, shown in a banner */
  errors: string[];
}

const DEFAULT_RPC: Record<Network, string> = {
  testnet: "https://rpc.testnet.fastnear.com",
  mainnet: "https://rpc.mainnet.fastnear.com",
};

export function resolveConfig(env: Record<string, string | undefined>): AppConfig {
  const errors: string[] = [];
  const network: Network = env.VITE_NETWORK === "mainnet" ? "mainnet" : "testnet";
  if (env.VITE_NETWORK && env.VITE_NETWORK !== "mainnet" && env.VITE_NETWORK !== "testnet") {
    errors.push(`VITE_NETWORK must be testnet or mainnet, got "${env.VITE_NETWORK}"`);
  }
  const core = (env.VITE_CORE ?? "").trim();
  const token = (env.VITE_TOKEN ?? "").trim();
  if (!core) errors.push("VITE_CORE is not set (core contract account).");
  if (!token) errors.push("VITE_TOKEN is not set (token contract account).");
  return {
    network,
    core,
    token,
    rpc: (env.VITE_RPC ?? "").trim() || DEFAULT_RPC[network],
    relayer: (env.VITE_RELAYER ?? "").trim() || null,
    staging: env.VITE_STAGING === "1",
    errors,
  };
}

export const config: AppConfig = resolveConfig(import.meta.env as Record<string, string | undefined>);
