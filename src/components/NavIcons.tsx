export type NavIconId =
  | "sessions"
  | "leaderboard"
  | "compare"
  | "fuel"
  | "settings";

const ICON_SRC: Record<NavIconId, string> = {
  sessions: "/icons/sessions.png",
  leaderboard: "/icons/leaderboard.png",
  compare: "/icons/compare.png",
  fuel: "/icons/fuel.png",
  settings: "/icons/settings.png",
};

export function NavIcon({ id }: { id: NavIconId }) {
  return (
    <span
      className="nav-icon-mask"
      style={{ maskImage: `url(${ICON_SRC[id]})`, WebkitMaskImage: `url(${ICON_SRC[id]})` }}
      role="img"
      aria-hidden
    />
  );
}
