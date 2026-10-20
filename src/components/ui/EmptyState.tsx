import { Button } from "./Button";

export interface EmptyAction {
  readonly label: string;
  readonly onClick: () => void;
}

interface EmptyStateProps {
  readonly message: string;
  readonly action: EmptyAction | null;
}

export function EmptyState({ message, action }: EmptyStateProps) {
  return (
    <div className="flex h-full min-h-48 flex-col items-center justify-center gap-4 px-6 py-12 text-center">
      <p className="max-w-sm text-[13px] leading-relaxed text-zinc-400">
        {message}
      </p>
      {action !== null && (
        <Button onClick={action.onClick}>{action.label}</Button>
      )}
    </div>
  );
}
