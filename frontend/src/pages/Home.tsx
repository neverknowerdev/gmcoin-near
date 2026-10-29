import { useState } from "react";
import { api } from "../api";
import { config } from "../lib/config";
import { useLoad } from "../hooks";
import { useWallet } from "../wallet";
import { Card, ErrorBox, Loading, Stat } from "../components/ui";
import { TxButton } from "../components/TxButton";
import { register, startXLink, verifyXLink } from "../lib/calls";
import { formatGm, formatNear, formatNsDate } from "../lib/format";
import { capStatus } from "../lib/cap";
import { intentUrl, linkPostText, profileUrl } from "../lib/x";

export function Home() {
  const wallet = useWallet();
  const account = wallet.accountId;
  const settings = useLoad(() => api.settings(), []);
  const user = useLoad(() => (account ? api.user(account) : Promise.resolve(null)), [account]);
  const registered = !!user.data;
  const pending = useLoad(
    () => (account && registered ? api.pendingLink(account) : Promise.resolve(null)),
    [account, registered],
  );
  const balance = useLoad(() => (account ? api.balance(account) : Promise.resolve(null)), [account]);
  const estimate = useLoad(
    () => (account && registered ? api.estimate(account) : Promise.resolve(null)),
    [account, registered],
  );

  const reloadAll = () => {
    user.reload();
    pending.reload();
    balance.reload();
    estimate.reload();
  };

  const [payMyself, setPayMyself] = useState(false);
  const [postUrl, setPostUrl] = useState("");

  const s = settings.data;
  const u = user.data;
  const cap = capStatus(u, s);
  const regDeposit = payMyself && s ? s.registerDeposit : null;
  const verifyDeposit = payMyself && s?.xVerifyDeposit != null ? s.xVerifyDeposit : null;

  return (
    <>
      <Card title="Say gm, earn GM">
        <p>
          Link your X account, post <b>gm</b> on X, and earn the GM token on NEAR. Everything below talks to{" "}
          <code>{config.core || "(no core configured)"}</code> on {config.network}.
        </p>
        {!wallet.ready && <Loading what="wallet" />}
        {wallet.ready && !account && (
          <button className="btn primary" onClick={wallet.signIn} disabled={config.errors.length > 0}>
            Connect wallet
          </button>
        )}
        {account && (
          <p>
            Connected as <b>{account}</b>{" "}
            <button className="btn link" onClick={wallet.signOut}>
              disconnect
            </button>
          </p>
        )}
        <ErrorBox error={wallet.error} />
      </Card>

      {account && (
        <>
          <ErrorBox error={user.error ?? settings.error} />

          {user.loading && !u && <Loading what="your account" />}

          {!user.loading && !user.error && !u && (
            <Card title="1. Register">
              <p>
                Registering creates your GM profile. It is free: core sponsors the storage within its daily caps (it may be full on a
                busy day).
              </p>
              <label className="check">
                <input type="checkbox" checked={payMyself} onChange={(e) => setPayMyself(e.target.checked)} />
                Pay storage myself{s ? ` (${formatNear(s.registerDeposit)} NEAR, refunded if you unregister)` : ""}
              </label>
              <TxButton
                className="btn primary"
                spec={register(config.core, regDeposit)}
                disabled={payMyself && !s}
                onDone={(ok) => ok && reloadAll()}
              >
                Register
              </TxButton>
              <p className="muted small">If you paid with a redirect wallet you will come back here after approving.</p>
            </Card>
          )}

          {u && (
            <>
              <Card title="Your GM" actions={<button className="btn link" onClick={reloadAll}>refresh</button>}>
                <div className="stats">
                  <Stat label="GM balance" value={balance.error ? "error" : formatGm(balance.data)} hint={balance.error} testId="balance" />
                  <Stat
                    label="Today's estimate"
                    value={estimate.data ? formatGm(estimate.data.estimate) : "-"}
                    hint={
                      estimate.data?.day != null
                        ? `${estimate.data.points} of ${estimate.data.dayPoints} points on day ${estimate.data.day}`
                        : "no scored day waiting to settle"
                    }
                  />
                  <Stat label="Accrued (lifetime)" value={formatGm(u.accrued)} />
                  <Stat label="Delivered (lifetime)" value={formatGm(u.delivered)} />
                </div>
                {u.accrued > u.delivered && (
                  <p className="muted small">{formatGm(u.accrued - u.delivered)} GM accrued but not delivered yet (mint pending or capped).</p>
                )}
                <ErrorBox error={estimate.error} />
              </Card>

              <Card title="$1 cap">
                {!cap && <Loading what="settings" />}
                {cap && !cap.capped && <p>You are verified as human: no lifetime cap on rewards.</p>}
                {cap && cap.capped && (
                  <>
                    <p>
                      Until World ID verification lands, unverified accounts can be minted at most{" "}
                      <b>{formatGm(cap.cap)} GM</b> in total (about $1).
                    </p>
                    <div className="meter" role="progressbar" aria-valuenow={cap.pct} aria-valuemin={0} aria-valuemax={100}>
                      <div style={{ width: `${cap.pct}%` }} className={cap.reached ? "full" : ""} />
                    </div>
                    <p className="small">
                      {formatGm(cap.minted)} of {formatGm(cap.cap)} GM used
                      {cap.reached ? " - cap reached, further rewards are not minted." : `, ${formatGm(cap.remaining)} left.`}
                    </p>
                  </>
                )}
              </Card>

              <Card title="2. Link X">
                {u.xId ? (
                  <p data-testid="x-linked">
                    Linked to{" "}
                    {u.xHandle ? (
                      <a href={profileUrl(u.xHandle)} target="_blank" rel="noreferrer">
                        @{u.xHandle}
                      </a>
                    ) : (
                      <>X id {u.xId}</>
                    )}
                    {u.linkedAt ? ` on ${formatNsDate(u.linkedAt)}` : ""}.
                  </p>
                ) : (
                  <>
                    {pending.error && <ErrorBox error={pending.error} />}
                    {!pending.data && (
                      <>
                        <p>Get a link code, post it on X, then verify.</p>
                        <TxButton className="btn primary" spec={startXLink(config.core)} onDone={(ok) => ok && reloadAll()}>
                          Get link code
                        </TxButton>
                      </>
                    )}
                    {pending.data && (
                      <>
                        <p>
                          Your link code: <code className="code" data-testid="link-code">{pending.data.code}</code>
                        </p>
                        <p>
                          <a className="btn" href={intentUrl(pending.data.code)} target="_blank" rel="noreferrer">
                            Post on X
                          </a>{" "}
                          <span className="muted small">posts &quot;{linkPostText(pending.data.code)}&quot;</span>
                        </p>
                        {pending.data.inFlight && <p className="alert info">Verification in progress. Check back in a minute.</p>}
                        {pending.data.lastError && <ErrorBox error={`Last verification failed: ${pending.data.lastError}`} />}
                        <label className="field">
                          Post link (optional, if search cannot find your post)
                          <input
                            value={postUrl}
                            onChange={(e) => setPostUrl(e.target.value)}
                            placeholder="https://x.com/you/status/123..."
                          />
                        </label>
                        <label className="check">
                          <input type="checkbox" checked={payMyself} onChange={(e) => setPayMyself(e.target.checked)} />
                          Pay the X check myself
                          {s?.xVerifyDeposit != null ? ` (${formatNear(s.xVerifyDeposit)} NEAR, unused part refunded)` : ""}
                        </label>
                        <TxButton
                          className="btn primary"
                          spec={() => verifyXLink(config.core, postUrl, verifyDeposit)}
                          disabled={pending.data.inFlight || (payMyself && s?.xVerifyDeposit == null)}
                          onDone={(ok) => {
                            reloadAll();
                            if (!ok) return;
                          }}
                        >
                          Verify (300 TGas)
                        </TxButton>
                        <p className="muted small">
                          Created {formatNsDate(pending.data.createdAt)}. Verification runs off-chain in a TEE and can take a minute;
                          press refresh above to see the result.
                        </p>
                        <TxButton className="btn link" spec={startXLink(config.core)} onDone={(ok) => ok && reloadAll()}>
                          New code
                        </TxButton>
                      </>
                    )}
                  </>
                )}
              </Card>
            </>
          )}
        </>
      )}
    </>
  );
}
