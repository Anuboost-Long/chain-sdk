import { Outlet } from "react-router-dom";

import NavBar from "../components/NavBar";

export default function RootLayout() {
  return (
    <div className="min-h-screen bg-chain-cream text-chain-navy dark:bg-chain-navy dark:text-chain-cream">
      <NavBar />
      <Outlet />
    </div>
  );
}
