import clsx from "clsx";

// One real SDK call, followed down through every layer and back up with
// its typed result. The only unprompted motion on the site: each joint
// lights in call order once on load, then the result appears. With
// reduced motion it renders straight in its final state.

const layers = [
  { name: "Your app", where: "React + TypeScript", does: "await desktop.platform.getInfo()", code: true },
  { name: "Chain SDK", where: "@chain/sdk", does: "Typed call, identical on every OS" },
  { name: "Chain Core", where: "Rust", does: "Routes it, turns native failures into ChainError" },
  { name: "Runtime", where: "Tauri, swappable", does: "Carries it across the native boundary" },
  { name: "Operating system", where: "macOS, Windows", does: "Answers with real system information" }
];

const stepMs = 280;

export function LayerTrace() {
  return (
    <figure className="relative">
      <figcaption className="sr-only">
        How desktop.platform.getInfo() travels from your app through Chain SDK, Chain Core, and the runtime to the
        operating system, and returns a typed PlatformInfo object.
      </figcaption>
      <ol className="relative">
        {layers.map((layer, index) => (
          <li key={layer.name} className="relative grid grid-cols-[1.25rem_1fr] gap-x-4">
            <div aria-hidden="true" className="relative flex flex-col items-center">
              <span
                className={clsx(
                  "mt-5 block h-4 w-2.5 rounded-[3px]",
                  "bg-line",
                  "animate-[joint-on_300ms_ease-out_both]"
                )}
                style={{ animationDelay: `${300 + index * stepMs}ms` }}
              />
              {index < layers.length - 1 && <span className="block w-px flex-1 bg-line" />}
            </div>
            <div
              className={clsx(
                "mb-2",
                "bg-raised",
                "border border-line rounded-xl",
                "px-4 py-3.5",
                "animate-[layer-on_300ms_ease-out_both]"
              )}
              style={{ animationDelay: `${300 + index * stepMs}ms` }}
            >
              <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-0.5">
                <span className="font-display text-ink text-[1.0625rem] font-semibold">{layer.name}</span>
                <span className="text-ink-faint text-[0.8125rem]">{layer.where}</span>
              </div>
              {layer.code ? (
                <code className="mt-1.5 block font-mono text-lime text-[0.8125rem] [@media(prefers-color-scheme:light)]:text-link">
                  {layer.does}
                </code>
              ) : (
                <p className="mt-1 text-ink-muted text-sm">{layer.does}</p>
              )}
            </div>
          </li>
        ))}
      </ol>
      <div
        className={clsx(
          "ml-9 mt-3",
          "bg-lime",
          "rounded-xl",
          "text-on-lime",
          "px-4 py-3.5",
          "animate-[result-in_400ms_ease-out_both]"
        )}
        style={{ animationDelay: `${300 + layers.length * stepMs + 150}ms` }}
      >
        <span className="block text-[0.8125rem] font-medium opacity-75">Returns PlatformInfo</span>
        <code className="mt-1 block font-mono text-[0.8125rem] font-medium">
          {'{ os: "macos", arch: "arm64", runtimeVersion: "2.11.5" }'}
        </code>
      </div>
    </figure>
  );
}
