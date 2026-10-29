import { config } from "./lib/config";
import { callView } from "./lib/rpc";
import {
  parseBalance,
  parseEpochs,
  parseEstimate,
  parseLeaderboard,
  parsePendingLink,
  parseSettings,
  parseStatus,
  parseUser,
} from "./lib/views";

const core = <T = unknown>(method: string, args: Record<string, unknown> = {}) =>
  callView<T>(config.rpc, config.core, method, args);

export const api = {
  status: async () => parseStatus(await core("status")),
  settings: async () => parseSettings(await core("get_settings")),
  epochHistory: async () => parseEpochs(await core("epoch_history", { from: 0, limit: 200 })),
  leaderboard: async (epoch: number) => parseLeaderboard(await core("leaderboard", { epoch })),
  user: async (account: string) => parseUser(await core("get_user", { account_id: account })),
  pendingLink: async (account: string) => parsePendingLink(await core("get_pending_link", { account_id: account })),
  estimate: async (account: string) => parseEstimate(await core("estimate_today", { account_id: account })),
  balance: async (account: string) =>
    parseBalance(await callView(config.rpc, config.token, "ft_balance_of", { account_id: account })),
};
