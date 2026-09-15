import clsx from "clsx";
import { NavLink } from "react-router-dom";

import chainIcon from "../../asset/app-icon.svg";

function navLinkClass({ isActive }: { readonly isActive: boolean }) {
  return clsx(
    "rounded-md px-3 py-1.5 text-sm font-medium",
    isActive
      ? "bg-chain-lime text-chain-navy"
      : "text-chain-navy/60 hover:bg-chain-navy/5 dark:text-chain-cream/60 dark:hover:bg-chain-cream/10"
  );
}

export default function NavBar() {
  return (
    <nav className="flex items-center gap-2 border-b border-chain-navy/10 px-4 py-2 dark:border-chain-cream/10">
      <img src={chainIcon} width={20} height={20} alt="" className="rounded-sm" />
      <NavLink to="/" end className={navLinkClass}>
        Home
      </NavLink>
      <NavLink to="/about" className={navLinkClass}>
        About
      </NavLink>
    </nav>
  );
}
