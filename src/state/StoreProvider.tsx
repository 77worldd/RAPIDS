import {
  createContext,
  useContext,
  useEffect,
  useReducer,
  type Dispatch,
  type ReactNode,
} from "react";
import { readSidebarCollapsed, writeSidebarCollapsed } from "../lib/uiPrefs";
import type { StoreStatus, Task, ViewId } from "../types";

export interface StoreState {
  readonly status: StoreStatus;
  readonly tasks: readonly Task[];
  readonly viewId: ViewId;
  readonly query: string;
  readonly sidebarCollapsed: boolean;
}

export type StoreAction =
  | { readonly type: "hydrate/start" }
  | { readonly type: "hydrate/ready"; readonly tasks: readonly Task[] }
  | { readonly type: "navigate"; readonly viewId: ViewId }
  | { readonly type: "search/set"; readonly query: string }
  | { readonly type: "sidebar/toggle" };

function reducer(state: StoreState, action: StoreAction): StoreState {
  switch (action.type) {
    case "hydrate/start":
      return { ...state, status: "loading" };
    case "hydrate/ready":
      return { ...state, status: "ready", tasks: action.tasks };
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
  };
}

/**
 * Loads persisted tasks into the store. The queue store attaches its read
 * path here; with no persisted tasks the store resolves empty.
 */
function loadTasks(): Promise<readonly Task[]> {
  return Promise.resolve([]);
}

const StateContext = createContext<StoreState | null>(null);
const DispatchContext = createContext<Dispatch<StoreAction> | null>(null);

export function StoreProvider({ children }: { readonly children: ReactNode }) {
  const [state, dispatch] = useReducer(reducer, undefined, createInitialState);

  useEffect(() => {
    if (state.status !== "loading") {
      return;
    }
    let active = true;
    void loadTasks().then((tasks) => {
      if (active) {
        dispatch({ type: "hydrate/ready", tasks });
      }
    });
    return () => {
      active = false;
    };
  }, [state.status]);

  useEffect(() => {
    writeSidebarCollapsed(state.sidebarCollapsed);
  }, [state.sidebarCollapsed]);

  return (
    <StateContext.Provider value={state}>
      <DispatchContext.Provider value={dispatch}>
        {children}
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
