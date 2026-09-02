import { useEffect } from "react";
import { Route, Routes } from "react-router-dom";
import { FuelCalculator } from "./views/FuelCalculator";
import { GlobalCompare } from "./views/GlobalCompare";
import { LapCompare } from "./views/LapCompare";
import { Leaderboard } from "./views/Leaderboard";
import { SessionReview } from "./views/SessionReview";
import { Sessions } from "./views/Sessions";
import { Settings } from "./views/Settings";
import { Sidebar } from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import { usePreferences } from "./lib/preferences";
import { applyTheme } from "./lib/theme";
import "./App.css";

export default function App() {
  const [prefs] = usePreferences();

  useEffect(() => {
    applyTheme(prefs.appearance);
  }, [prefs.appearance]);

  return (
    <div className="app-shell">
      {/* Sidebar precedes TitleBar so the title bar's rail can follow its
          hover expansion; grid placement keeps the title bar on top. */}
      <Sidebar />
      <TitleBar />
      <main className="content">
        <Routes>
          <Route path="/" element={<Sessions />} />
          <Route path="/sessions/:sessionId" element={<SessionReview />} />
          <Route path="/leaderboard" element={<Leaderboard />} />
          <Route path="/fuel" element={<FuelCalculator />} />
          <Route path="/compare" element={<GlobalCompare />} />
          <Route path="/compare/:sessionId" element={<LapCompare />} />
          <Route path="/settings" element={<Settings />} />
        </Routes>
      </main>
    </div>
  );
}
