import { viewLabel } from "../../config/navigation";
import { countForView } from "../../state/selectors";
import { useStoreState } from "../../state/StoreProvider";

export function ViewTitle() {
  const { tasks, viewId } = useStoreState();
  const count = countForView(tasks, viewId);
  return (
    <div className="flex min-w-0 items-baseline gap-2">
      <h1 className="truncate text-[14px] font-semibold text-zinc-100">
        {viewLabel(viewId)}
      </h1>
      <span
        className="text-[13px] tabular-nums text-zinc-500"
        aria-hidden="true"
      >
        {count}
      </span>
    </div>
  );
}
