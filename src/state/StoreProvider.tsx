import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useReducer,
  useRef,
  type Dispatch,
  type ReactNode,
} from "react";
import {
  describeError,
  listTasks,
  queuePaused,
  subscribeHostEvents,
} from "../lib/ipc";
import { readSidebarCollapsed, writeSidebarCollapsed } from "../lib/uiPrefs";
import type { HostEvent, StoreStatus, Task, ViewId } from "../types";

export interface StoreState {
  readonly status: StoreStatus;
  readonly tasks: readonly Task[];
  readonly viewId: ViewId;
  readonly query: string;
  readonly sidebarCollapsed: boolean;
  readonly queuePaused: boolean;
  readonly notice: string | null;
}

export type StoreAction =
  | {
      readonly type: "hydrate/ready";
      readonly tasks: readonly Task[];
      readonly paused: boolean;
      readonly events: readonly HostEvent[];
    }
  | { readonly type: "hydrate/failed"; readonly message: string }
  | { readonly type: "host/event"; readonly event: HostEvent }
  | { readonly type: "queue/state"; readonly paused: boolean }
  | { readonly type: "notice/set"; readonly message: string }
  | { readonly type: "notice/clear" }
  | { readonly type: "navigate"; readonly viewId: ViewId }
  | { readonly type: "search/set"; readonly query: string }
  | { readonly type: "sidebar/toggle" };

/** Apply only what the host asserts; progress is display-only, never a state transition. */
function applyEvent(state: StoreState, event: HostEvent): StoreState {
  switch (event.kind) {
    case "taskChanged": {
      // Wall-clock timestamps can move backwards; event arrival, not
      // updatedAt, determines the authoritative transition order.
      const earlier = state.tasks.find((task) => task.id === event.task.id);
      if (earlier === undefined) {
        return { ...state, tasks: [event.task, ...state.tasks] };
      }
      return {
        ...state,
        tasks: state.tasks.map((task) =>
          task.id === event.task.id ? event.task : task,
        ),
      };
    }
    case "taskRemoved":
      return {
        ...state,
        tasks: state.tasks.filter((task) => task.id !== event.id),
      };
    case "progress": {
      const byId = new Map(event.updates.map((update) => [update.id, update]));
      return {
        ...state,
        tasks: state.tasks.map((task) => {
          const update = byId.get(task.id);
          if (update === undefined || task.status !== "downloading") return task;
          return {
            ...task,
            downloaded: update.downloaded,
            speedBps: update.speedBps,
            etaSeconds: update.etaSeconds,
            totalBytes: update.totalBytes,
          };
        }),
      };
    }
    case "queueChanged":
      return state; // A separate queue_paused read supplies the actual state.
  }
}

function reducer(state: StoreState, action: StoreAction): StoreState {
  switch (action.type) {
    case "hydrate/ready": {
      const snapshot: StoreState = {
        ...state,
        status: "ready",
        tasks: action.tasks,
        queuePaused: action.paused,
        notice: state.status === "failed" ? null : state.notice,
      };
      return action.events.reduce(applyEvent, snapshot);
    }
    case "hydrate/failed":
      return {
        ...state,
        status: state.status === "ready" ? "ready" : "failed",
        notice: action.message,
      };
    case "host/event":
      return applyEvent(state, action.event);
    case "queue/state":
      return { ...state, queuePaused: action.paused };
    case "notice/set":
      return { ...state, notice: action.message };
    case "notice/clear":
      return { ...state, notice: null };
    case "navigate":
      return { ...state, viewId: action.viewId };
    case "search/set":
      return { ...state, query: action.query };
    case "sidebar/toggle":
      return { ...state, sidebarCollapsed: !state.sidebarCollapsed };
  }
}

function createInitialState(): StoreState {
  return {
    status: "loading",
    tasks: [],
    viewId: "all",
    query: "",
    sidebarCollapsed: readSidebarCollapsed(),
    queuePaused: false,
    notice: null,
  };
}

const StateContext = createContext<StoreState | null>(null);
const DispatchContext = createContext<Dispatch<StoreAction> | null>(null);
const RefreshContext = createContext<(() => void) | null>(null);

export function StoreProvider({ children }: { readonly children: ReactNode }) {
  const [state, dispatch] = useReducer(reducer, undefined, createInitialState);
  const refreshRef = useRef<() => void>(() => {});
  const refresh = useCallback(() => {
    refreshRef.current();
  }, []);

  useEffect(() => {
    let active = true;
    let connected = false;
    let connecting = false;
    let unlisten: (() => void) | null = null;
    let requestId = 0;
    let queueReadId = 0;
    let buffering: HostEvent[] | null = null;

    const readQueue = (): void => {
      const read = ++queueReadId;
      void queuePaused()
        .then((paused) => {
          if (active && read === queueReadId) {
            dispatch({ type: "queue/state", paused });
          }
        })
        .catch((error: unknown) => {
          if (active && read === queueReadId) {
            dispatch({ type: "notice/set", message: describeError(error) });
          }
        });
    };

    const receive = (event: HostEvent): void => {
      if (!active) return;
      if (buffering !== null) {
        buffering.push(event);
      } else if (event.kind === "queueChanged") {
        readQueue();
      } else {
        dispatch({ type: "host/event", event });
      }
    };

    const load = async (alreadyReceived: HostEvent[] = []): Promise<void> => {
      const request = ++requestId;
      ++queueReadId; // A previous queue-state read cannot overwrite this snapshot.
      buffering = alreadyReceived;
      try {
        const [tasks, paused] = await Promise.all([listTasks(), queuePaused()]);
        if (!active || request !== requestId) return;
        dispatch({ type: "hydrate/ready", tasks, paused, events: alreadyReceived });
        if (alreadyReceived.some((event) => event.kind === "queueChanged")) {
          readQueue();
        }
      } catch (error) {
        if (active && request === requestId) {
          for (const event of alreadyReceived) {
            if (event.kind === "queueChanged") readQueue();
            else dispatch({ type: "host/event", event });
          }
          dispatch({ type: "hydrate/failed", message: describeError(error) });
        }
      } finally {
        if (buffering === alreadyReceived) buffering = null;
      }
    };

    const connect = async (): Promise<void> => {
      if (!active || connecting) return;
      connecting = true;
      const beforeSnapshot: HostEvent[] = [];
      buffering = beforeSnapshot;
      try {
        const stop = await subscribeHostEvents(receive, (error) => {
          if (active) {
            dispatch({ type: "notice/set", message: describeError(error) });
          }
        });
        if (!active) {
          stop();
          return;
        }
        unlisten = stop;
        connected = true;
        await load(beforeSnapshot);
      } catch (error) {
        if (active) {
          dispatch({ type: "hydrate/failed", message: describeError(error) });
        }
      } finally {
        connecting = false;
        if (buffering === beforeSnapshot) buffering = null;
      }
    };

    refreshRef.current = () => {
      if (!active) return;
      if (connected) void load();
      else void connect();
    };
    void connect();

    return () => {
      active = false;
      ++requestId;
      ++queueReadId;
      unlisten?.();
      refreshRef.current = () => {};
    };
  }, []);

  useEffect(() => {
    writeSidebarCollapsed(state.sidebarCollapsed);
  }, [state.sidebarCollapsed]);

  return (
    <StateContext.Provider value={state}>
      <DispatchContext.Provider value={dispatch}>
        <RefreshContext.Provider value={refresh}>
          {children}
        </RefreshContext.Provider>
      </DispatchContext.Provider>
    </StateContext.Provider>
  );
}

export function useStoreState(): StoreState {
  const state = useContext(StateContext);
  if (state === null) {
    throw new Error("useStoreState must be used inside StoreProvider");
  }
  return state;
}

export function useStoreDispatch(): Dispatch<StoreAction> {
  const dispatch = useContext(DispatchContext);
  if (dispatch === null) {
    throw new Error("useStoreDispatch must be used inside StoreProvider");
  }
  return dispatch;
}

export function useRefreshTasks(): () => void {
  const refresh = useContext(RefreshContext);
  if (refresh === null) {
    throw new Error("useRefreshTasks must be used inside StoreProvider");
  }
  return refresh;
}

export function useCommand(): (operation: () => Promise<unknown>) => void {
  const dispatch = useStoreDispatch();
  return useCallback(
    (operation: () => Promise<unknown>) => {
      void operation().catch((error: unknown) => {
        dispatch({ type: "notice/set", message: describeError(error) });
      });
    },
    [dispatch],
  );
}
