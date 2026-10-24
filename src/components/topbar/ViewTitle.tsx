import { viewLabel } from "../../config/navigation";
import { countForView } from "../../state/selectors";
import { useStoreState } from "../../state/StoreProvider";

export function ViewTitle() {
  const { tasks, viewId } = useStoreState();
  const count = countForView(tasks, viewId);
  const active = tasks.filter((task) => task.status === "downloading").length;
  const waiting = tasks.filter(
    (task) => task.status === "queued" || task.status === "scheduled",
  ).length;

  return (
    <div className="flex min-w-0 shrink-0 flex-col justify-center">
      <div className="flex min-w-0 items-baseline gap-2">
        <h1 className="truncate text-[14px] font-semibold tracking-[-0.01em] text-high">
          {viewLabel(viewId)}
        </h1>
        <span
          className="text-[11.5px] tabular-nums text-zinc-500"
          aria-label={`${count} downloads`}
        >
          {count}
        </span>
      </div>
      <p className="mt-0.5 truncate text-[10.5px] text-zinc-500">
        {active > 0 || waiting > 0
          ? `${active} active · ${waiting} waiting`
          : "Queue is idle"}
      </p>
    </div>
  );
}
