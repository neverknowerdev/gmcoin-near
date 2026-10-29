import { useCallback, useEffect, useRef, useState } from "react";

export interface Loaded<T> {
  data: T | null;
  error: string | null;
  loading: boolean;
  reload(): void;
}

/** Runs an async loader on mount and when `deps` change; never throws. */
export function useLoad<T>(loader: () => Promise<T>, deps: unknown[]): Loaded<T> {
  const [state, setState] = useState<{ data: T | null; error: string | null; loading: boolean }>({
    data: null,
    error: null,
    loading: true,
  });
  const [tick, setTick] = useState(0);
  const loaderRef = useRef(loader);
  loaderRef.current = loader;

  useEffect(() => {
    let live = true;
    setState((s) => ({ ...s, loading: true }));
    loaderRef
      .current()
      .then((data) => live && setState({ data, error: null, loading: false }))
      .catch((e) => live && setState({ data: null, error: e instanceof Error ? e.message : String(e), loading: false }));
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, tick]);

  const reload = useCallback(() => setTick((t) => t + 1), []);
  return { ...state, reload };
}
