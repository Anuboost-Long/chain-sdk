import clsx from "clsx";

// Same geometry as asset/chain-mark.svg. On dark it keeps the fixed lime
// and pale sections; on light it falls back to the monochrome variant,
// as the brand guide asks when lime lacks contrast.
export function ChainMark({ className }: Readonly<{ className?: string }>) {
  return (
    <svg viewBox="40 40 172 176" aria-hidden="true" className={clsx("shrink-0", className)}>
      <path
        className="fill-lime [@media(prefers-color-scheme:light)]:fill-ink"
        d="M204 48H108C74.863 48 48 74.863 48 108V116H64Q68 116 68 120V136Q68 140 72 140H80Q84 140 84 136V120Q84 116 88 116H104V108Q104 96 116 96H184L204 76V48Z"
      />
      <path
        className="fill-ink"
        d="M48 124H56Q60 124 60 128V144Q60 148 64 148H88Q92 148 92 144V128Q92 124 96 124H104V148Q104 160 116 160H184L204 180V208H108C74.863 208 48 181.137 48 148V124Z"
      />
    </svg>
  );
}
