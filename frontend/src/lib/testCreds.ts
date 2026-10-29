export const TEST_ACCOUNT_KEY = "gm_test_account";
export const TEST_PRIVATE_KEY = "gm_test_key";

export interface TestCreds {
  accountId: string;
  privateKey: string;
}

/** Validates the staging test credentials read from localStorage. */
export function parseTestCreds(account: string | null, key: string | null): TestCreds {
  const accountId = (account ?? "").trim();
  const privateKey = (key ?? "").trim();
  if (!accountId || !privateKey) {
    throw new Error(`Set localStorage ${TEST_ACCOUNT_KEY} and ${TEST_PRIVATE_KEY} to use the test key wallet.`);
  }
  if (!/^[a-z0-9._-]{2,64}$/.test(accountId)) throw new Error(`Bad test account id "${accountId}".`);
  if (!/^ed25519:[1-9A-HJ-NP-Za-km-z]{60,100}$/.test(privateKey)) {
    throw new Error("Test key must be an ed25519:<base58> private key.");
  }
  return { accountId, privateKey };
}
