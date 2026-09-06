import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { subscribeRealtime } from "../generated/client";
import { resourceQueryKeys } from "../query";

export interface RealtimeResourceOptions {
  enabled: boolean;
  resourceId: string;
  resourceSlug: string;
  eventPrefix: string;
}

export function useRealtimeResource({
  enabled,
  resourceId,
  resourceSlug,
  eventPrefix,
}: RealtimeResourceOptions): void {
  const queryClient = useQueryClient();

  useEffect(() => {
    if (!enabled) return;
    const source = subscribeRealtime({ resource: resourceSlug });
    let refreshTimer: ReturnType<typeof setTimeout> | undefined;
    const refresh = () => {
      if (refreshTimer !== undefined) return;
      refreshTimer = setTimeout(() => {
        refreshTimer = undefined;
        void queryClient.invalidateQueries({
          queryKey: resourceQueryKeys.all(resourceId),
        });
      }, 50);
    };
    const events = [
      `${eventPrefix}.created`,
      `${eventPrefix}.updated`,
      `${eventPrefix}.deleted`,
      "resync",
    ];
    for (const event of events) source.addEventListener(event, refresh);
    return () => {
      source.close();
      if (refreshTimer !== undefined) clearTimeout(refreshTimer);
    };
  }, [enabled, eventPrefix, queryClient, resourceId, resourceSlug]);
}
