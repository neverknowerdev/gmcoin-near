export type Route = "home" | "leaderboard" | "budget" | "bell" | "lab";

export interface NavItem {
  route: Route;
  label: string;
  hash: string;
}

export function navItems(staging: boolean): NavItem[] {
  const items: NavItem[] = [
    { route: "home", label: "Home", hash: "#/" },
    { route: "leaderboard", label: "Leaderboard", hash: "#/leaderboard" },
    { route: "budget", label: "Budget", hash: "#/budget" },
    { route: "bell", label: "Ring the bell", hash: "#/bell" },
  ];
  if (staging) items.push({ route: "lab", label: "Lab", hash: "#/lab" });
  return items;
}

export function parseRoute(hash: string, staging: boolean): Route {
  const path = hash.replace(/^#\/?/, "").split("?")[0].replace(/\/$/, "");
  switch (path) {
    case "leaderboard":
      return "leaderboard";
    case "budget":
      return "budget";
    case "bell":
      return "bell";
    case "lab":
      return staging ? "lab" : "home";
    default:
      return "home";
  }
}
