import { Sidebar } from "../sidebar/Sidebar";
import { TopBar } from "../topbar/TopBar";
import { TasksView } from "../../views/TasksView";

export function AppShell() {
  return (
    <div className="flex h-screen w-screen overflow-hidden bg-zinc-950">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        <TopBar />
        <main className="min-h-0 flex-1 overflow-y-auto">
          <TasksView />
        </main>
      </div>
    </div>
  );
}
