import createMDX from "@next/mdx";
import type { NextConfig } from "next";

import { chainDark, chainLight } from "./lib/code-themes";

const nextConfig: NextConfig = {
  // A static export: every page is prebuilt HTML, and Next's router takes
  // over after the first load so moving between pages never reloads.
  output: "export",
  trailingSlash: true,
  images: { unoptimized: true },
  pageExtensions: ["ts", "tsx", "md", "mdx"]
};

const withMDX = createMDX({
  options: {
    remarkPlugins: ["remark-gfm"],
    rehypePlugins: [
      "rehype-slug",
      ["rehype-pretty-code", { theme: { dark: chainDark, light: chainLight }, keepBackground: false }]
    ]
  }
});

export default withMDX(nextConfig);
