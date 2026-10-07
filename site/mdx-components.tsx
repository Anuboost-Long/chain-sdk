import type { MDXComponents } from "mdx/types";
import Link from "next/link";
import type { ComponentProps } from "react";

// Internal links go through next/link so moving between docs pages is a
// client-side route change, never a full reload.
function MdxLink({ href = "", ...rest }: Readonly<ComponentProps<"a">>) {
  if (href.startsWith("/") || href.startsWith("#")) return <Link href={href} {...rest} />;
  return <a href={href} {...rest} />;
}

const components: MDXComponents = { a: MdxLink };

export function useMDXComponents(): MDXComponents {
  return components;
}
