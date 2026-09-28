import { useEffect, type RefObject } from "react";

/** Closes a modal <dialog> when its backdrop is clicked. Esc already closes it natively. */
export function useBackdropClose(dialog: RefObject<HTMLDialogElement | null>) {
  useEffect(() => {
    const element = dialog.current;
    if (!element) return;
    const onClick = (event: MouseEvent) => {
      if (event.target === element) element.close();
    };
    element.addEventListener("click", onClick);
    return () => element.removeEventListener("click", onClick);
  }, [dialog]);
}
