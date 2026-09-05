import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { subscribeRealtime } from "../generated/client";
import { appQueryKeys } from "../query";
import type { ResourceDefinition } from "../resource";

const MAX_SEEN_EVENTS = 256;

export function useActivityRealtime(
  enabled: boolean,
  resource: ResourceDefinition,
  recordId: string,
): void {
  const queryClient = useQueryClient();

  useEffect(() => {
    if (!enabled) return;
    const source = subscribeRealtime({
      resource: resource.slug,
      recordId,
    });
    const seen = new Set<string>();
    const refresh = (event: Event) => {
      const eventId = (event as MessageEvent<string>).lastEventId;
      if (eventId && seen.has(eventId)) return;
      if (eventId) {
        seen.add(eventId);
        if (seen.size > MAX_SEEN_EVENTS) {
          const oldest = seen.values().next().value;
          if (oldest) seen.delete(oldest);
        }
      }
      void queryClient.invalidateQueries({
        queryKey: appQueryKeys.activity(resource.slug, recordId),
      });
    };
    const events = [
      "activity.comment.created",
      "activity.comment.withdrawn",
      "activity.comment.moderated",
      `${resource.eventPrefix}.created`,
      `${resource.eventPrefix}.updated`,
      `${resource.eventPrefix}.deleted`,
      ...(resource.workflow?.transitions.map(
        (transition) =>
          `${resource.eventPrefix}.workflow.${transition.name}`,
      ) ?? []),
      "resync",
    ];
    for (const event of events) source.addEventListener(event, refresh);
    return () => source.close();
  }, [enabled, queryClient, recordId, resource]);
}
