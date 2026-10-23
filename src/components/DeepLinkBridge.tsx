import { useEffect } from "react";
import { createAddDraft, toAddRequest } from "../lib/addDraft";
import type { PendingDeepLink } from "../lib/deeplink";
import { inspectUrl, suggestedCategory } from "../lib/fileMeta";
import {
  addTask,
  describeError,
  subscribeDeepLinkSignals,
  takePendingDeepLinks,
} from "../lib/ipc";
import { readDeepLinkConfirm } from "../lib/preferences";
import { useAddDownload } from "../state/AddDownloadProvider";
import { useRefreshTasks, useStoreDispatch } from "../state/StoreProvider";

/**
 * Drains the host's browser inbox for cold starts and re-drains whenever a
 * validated warm link arrives. The add sheet stays the default; direct queueing
 * happens only after the user turns the confirmation setting off.
 */
export function DeepLinkBridge() {
  const { openHandoff } = useAddDownload();
  const dispatch = useStoreDispatch();
  const refresh = useRefreshTasks();

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    let sequence: Promise<void> = Promise.resolve();

    async function apply(items: readonly PendingDeepLink[]): Promise<void> {
      for (const item of items) {
        if (disposed) return;
        if (item.kind === "error") {
          dispatch({ type: "notice/set", message: item.reason });
          continue;
        }
        if (readDeepLinkConfirm()) {
          openHandoff(item.request);
          continue;
        }
        const info = inspectUrl(item.request.url);
        try {
          await addTask(
            toAddRequest({
              ...createAddDraft(),
              url: item.request.url,
              category: suggestedCategory(info),
            }),
          );
          if (disposed) return;
          refresh();
          dispatch({ type: "navigate", viewId: "all" });
          dispatch({ type: "search/set", query: "" });
          dispatch({
            type: "notice/set",
            message: `Added ${info.fileName ?? item.request.name ?? "the browser download"} from the browser.`,
          });
        } catch (cause) {
          if (disposed) return;
          dispatch({
            type: "notice/set",
            message: `The browser link could not be added. ${describeError(cause)}`,
          });
        }
      }
    }

    function drain(): void {
      sequence = sequence.then(async () => {
        try {
          const items = await takePendingDeepLinks();
          if (!disposed) await apply(items);
        } catch (cause) {
          if (!disposed) {
            dispatch({ type: "notice/set", message: describeError(cause) });
          }
        }
      });
    }

    void subscribeDeepLinkSignals(drain)
      .then((off) => {
        if (disposed) {
          off();
          return;
        }
        unlisten = off;
        drain();
      })
      .catch((cause: unknown) => {
        if (!disposed) {
          dispatch({
            type: "notice/set",
            message: `Browser handoffs are unavailable. ${describeError(cause)}`,
          });
        }
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [dispatch, openHandoff, refresh]);

  return null;
}
