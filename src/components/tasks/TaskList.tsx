import { useEffect, useRef } from "react";
import { useTaskActions } from "../../state/useTaskActions";
import type { Task } from "../../types";
import { TaskRow } from "./TaskRow";

interface TaskListProps {
  readonly tasks: readonly Task[];
  readonly selected: ReadonlySet<number>;
  readonly onToggle: (id: number) => void;
  readonly onToggleAll: (selected: boolean) => void;
}

export function TaskList({
  tasks,
  selected,
  onToggle,
  onToggleAll,
}: TaskListProps) {
  const actions = useTaskActions();
  const all = useRef<HTMLInputElement>(null);
  const allSelected = tasks.length > 0 && tasks.every((task) => selected.has(task.id));
  const someSelected = tasks.some((task) => selected.has(task.id)) && !allSelected;

  useEffect(() => {
    if (all.current !== null) {
      all.current.indeterminate = someSelected;
    }
  }, [someSelected]);

  return (
    <div role="table" aria-label="Downloads" className="flex h-full min-h-0 flex-col">
      <div
        role="rowgroup"
        className="shrink-0 border-b border-line/80 bg-canvas/55 backdrop-blur"
      >
        <div
          role="row"
          className="flex h-9 items-center gap-2 px-3 text-[9.5px] font-semibold uppercase tracking-[0.09em] text-zinc-600 min-[1180px]:gap-3 min-[1180px]:px-4"
        >
          <div className="flex w-5 shrink-0 items-center justify-center">
            <input
              ref={all}
              type="checkbox"
              checked={allSelected}
              onChange={() => onToggleAll(!allSelected)}
              aria-label={allSelected ? "Clear selection" : "Select all downloads"}
              className="h-3.5 w-3.5 cursor-pointer rounded border-zinc-600 bg-transparent accent-emerald-500"
            />
          </div>
          <span role="columnheader" className="min-w-0 flex-1">
            File
          </span>
          <span role="columnheader" className="w-16 shrink-0 text-right min-[1180px]:w-[4.5rem]">
            Size
          </span>
          <span role="columnheader" className="w-[7rem] shrink-0 min-[1180px]:w-40">
            Progress
          </span>
          <span role="columnheader" className="w-[3.75rem] shrink-0 text-right min-[1180px]:w-[4.2rem]">
            Speed
          </span>
          <span role="columnheader" className="w-[3.25rem] shrink-0 text-right min-[1180px]:w-14">
            ETA
          </span>
          <span role="columnheader" className="w-24 shrink-0 min-[1180px]:w-28">
            Status
          </span>
          <span role="columnheader" className="w-[3.6rem] shrink-0 text-right">
            Actions
          </span>
        </div>
      </div>
      <div role="rowgroup" className="min-h-0 flex-1 overflow-y-auto">
        {tasks.map((task) => (
          <TaskRow
            key={task.id}
            task={task}
            selected={selected.has(task.id)}
            onToggle={onToggle}
            actions={actions}
          />
        ))}
      </div>
    </div>
  );
}
