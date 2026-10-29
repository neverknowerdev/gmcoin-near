import { useEffect, useState } from "react";
import { WalletProvider, useWallet } from "./wallet";
import { config } from "./lib/config";
import { navItems, parseRoute, type Route } from "./lib/route";
import { Home } from "./pages/Home";
import { Leaderboard } from "./pages/Leaderboard";
import { Budget } from "./pages/Budget";
import { Bell } from "./pages/Bell";
import { Lab } from "./pages/Lab";
import { ErrorBox } from "./components/ui";
import { shortAccount } from "./lib/format";

function useRoute(): Route {
  const [route, setRoute] = useState<Route>(() => parseRoute(location.hash, config.staging));
  useEffect(() => {
    const on = () => setRoute(parseRoute(location.hash, config.staging));
    window.addEventListener("hashchange", on);
    return () => window.removeEventListener("hashchange", on);
  }, []);
  return route;
}

/** Redirect wallets return with ?transactionHashes= or ?errorMessage=. Show it once, then clean the URL. */
function useWalletReturn(): { ok: boolean; text: string } | null {
  const [note] = useState(() => {
    try {
      const q = new URLSearchParams(location.search);
      const err = q.get("errorMessage");
      const hash = q.get("transactionHashes");
      if (!err && !hash) return null;
      history.replaceState(null, "", location.pathname + location.hash);
      return err ? { ok: false, text: `Wallet: ${decodeURIComponent(err)}` } : { ok: true, text: `Transaction sent (${hash}). Refresh the page data if it has not updated.` };
    } catch {
      return null;
    }
  });
  return note;
}

function Shell() {
  const route = useRoute();
  const wallet = useWallet();
  const back = useWalletReturn();
  const items = navItems(config.staging);
  return (
    <div className="app">
      <header className="top">
        <a className="brand" href="#/">GM Coin <span className="muted">on NEAR</span></a>
        <nav aria-label="Main">
          {items.map((i) => (
            <a key={i.route} href={i.hash} className={i.route === route ? "active" : ""} aria-current={i.route === route ? "page" : undefined}>
              {i.label}
            </a>
          ))}
        </nav>
        <div className="who">
          {wallet.accountId ? (
            <span title={wallet.accountId}>{shortAccount(wallet.accountId, 20)}</span>
          ) : (
            <button className="btn small" onClick={wallet.signIn} disabled={!wallet.ready || config.errors.length > 0}>Connect</button>
          )}
        </div>
      </header>
      <main>
        {config.errors.map((e) => <ErrorBox key={e} error={e} />)}
        {back && <div className={`alert ${back.ok ? "info" : "error"}`}>{back.text}</div>}
        {!config.errors.length && (
          <>
            {route === "home" && <Home />}
            {route === "leaderboard" && <Leaderboard />}
            {route === "budget" && <Budget />}
            {route === "bell" && <Bell />}
            {route === "lab" && <Lab />}
          </>
        )}
      </main>
      <footer>
        {config.network} · core <code>{config.core || "?"}</code> · token <code>{config.token || "?"}</code>
      </footer>
    </div>
  );
}

export function App() {
  return (
    <WalletProvider>
      <Shell />
    </WalletProvider>
  );
}
