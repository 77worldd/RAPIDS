import { Search, X } from "lucide-react";
import { useEffect, useRef } from "react";
import { useStoreDispatch, useStoreState } from "../../state/StoreProvider";

export function SearchBox() {
  const { query } = useStoreState();
  const dispatch = useStoreDispatch();
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent): void => {
      if (
        event.key !== "/" ||
        event.ctrlKey ||
        event.metaKey ||
        event.altKey ||
        event.defaultPrevented
      ) {
        return;
      }
      const target = event.target;
      if (
        target instanceof HTMLElement &&
        (target.isContentEditable ||
          target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.tagName === "SELECT")
      ) {
        return;
      }
      event.preventDefault();
      input.current?.focus();
      input.current?.select();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const clearSearch = () => dispatch({ type: "search/set", query: "" });

  return (
    <div className="relative w-[17rem] min-w-36 shrink max-[1240px]:w-52 max-[1100px]:w-40">
      <Search
        size={14}
        strokeWidth={1.8}
        className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-zinc-500"
      />
      {query === "" && (
        <span className="pointer-events-none absolute inset-y-0 left-8 right-9 flex items-center truncate text-[12.5px] text-zinc-500">
          Search downloads
        </span>
      )}
      <input
        ref={input}
        type="text"
        value={query}
        onChange={(event) =>
          dispatch({ type: "search/set", query: event.target.value })
        }
        onKeyDown={(event) => {
          if (event.key === "Escape" && query !== "") {
            clearSearch();
          }
        }}
        aria-label="Search downloads"
        aria-keyshortcuts="/"
        className="h-8 w-full rounded-lg border border-line bg-canvas/55 pl-8 pr-14 text-[12.5px] text-zinc-200 outline-none transition-[border-color,background-color,box-shadow] duration-150 ease-out focus:border-emerald-500/55 focus:bg-canvas focus:ring-2 focus:ring-emerald-500/15"
      />
      {query !== "" ? (
        <button
          type="button"
          onClick={clearSearch}
          aria-label="Clear search"
          className="absolute right-1.5 top-1/2 inline-flex h-6 w-6 -translate-y-1/2 items-center justify-center rounded text-zinc-500 transition-colors hover:bg-white/[0.06] hover:text-zinc-200 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-emerald-500/60"
        >
          <X size={12} strokeWidth={2} />
        </button>
      ) : (
        <kbd className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 rounded border border-line bg-panel px-1.5 py-0.5 text-[9px] text-zinc-500">
          /
        </kbd>
      )}
    </div>
  );
}
