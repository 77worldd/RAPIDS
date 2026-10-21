import { Search, X } from "lucide-react";
import { useStoreDispatch, useStoreState } from "../../state/StoreProvider";

export function SearchBox() {
  const { query } = useStoreState();
  const dispatch = useStoreDispatch();

  const clearSearch = () => dispatch({ type: "search/set", query: "" });

  return (
    <div className="relative w-64 min-w-32 shrink">
      <Search
        size={13}
        strokeWidth={1.75}
        className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-zinc-500"
      />
      <input
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
        className="h-7 w-full rounded-md border border-zinc-800 bg-zinc-900/70 pl-8 pr-7 text-[13px] text-zinc-200 transition-colors duration-150 ease-out outline-none focus:border-emerald-600/60 focus:bg-zinc-900"
      />
      {query !== "" && (
        <button
          type="button"
          onClick={clearSearch}
          aria-label="Clear search"
          title="Clear search"
          className="absolute right-1 top-1/2 flex h-5 w-5 -translate-y-1/2 items-center justify-center rounded text-zinc-500 transition-colors duration-150 ease-out hover:bg-zinc-800 hover:text-zinc-200 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-emerald-500/70"
        >
          <X size={12} strokeWidth={2} />
        </button>
      )}
    </div>
  );
}
