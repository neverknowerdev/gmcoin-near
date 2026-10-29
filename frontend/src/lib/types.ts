/** Shapes of core views after parsing. U64/U128 JSON strings become bigint. */

export interface Status {
  now: bigint;
  today: number;
  currentEpoch: number;
  budget: bigint;
  dailyPool: bigint;
  scanDay: number;
  settleDay: number;
  scanDue: boolean;
  settleDue: boolean;
  closeDue: boolean;
  users: number;
  linked: number;
  totalMinted: bigint;
  paused: boolean;
  failedMints: number;
}

export interface Epoch {
  index: number;
  startDay: number;
  budget: bigint;
  dailyPool: bigint;
  activeUsers: number;
  totalPoints: number;
  minted: bigint;
  closed: boolean;
}

export interface User {
  id: string;
  nearAccount: string;
  xId: string | null;
  xHandle: string | null;
  accrued: bigint;
  delivered: bigint;
  mintedUnverified: bigint;
  verifiedHuman: boolean;
  sponsored: boolean;
  storageDeposit: bigint;
  linkedAt: bigint | null;
  lastScoredDay: number | null;
  lastDayPoints: number;
  epochPoints: number;
}

export interface PendingLink {
  code: string;
  createdAt: bigint;
  inFlight: boolean;
  lastError: string | null;
}

export interface Estimate {
  day: number | null;
  points: number;
  dayPoints: number;
  estimate: bigint;
}

export interface Settings {
  owner: string;
  token: string;
  treasury: string;
  registerDeposit: bigint;
  unverifiedCap: bigint;
  tipAmount: bigint;
  tipDailyLimit: bigint;
  xVerifyDeposit: bigint | null;
  dayNs: bigint;
  daysPerEpoch: number;
}

export interface LeaderRow {
  account: string;
  handle: string | null;
  points: number;
}
