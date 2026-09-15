import { createBrowserRouter } from "react-router-dom";

import RootLayout from "./layouts/RootLayout";
import About from "./pages/About";
import Home from "./pages/Home";

// Add new top-level pages as siblings of Home/About here; nest under a
// parent route only when pages genuinely share layout beyond RootLayout.
export const router = createBrowserRouter([
  {
    path: "/",
    element: <RootLayout />,
    children: [
      { index: true, element: <Home /> },
      { path: "about", element: <About /> }
    ]
  }
]);
