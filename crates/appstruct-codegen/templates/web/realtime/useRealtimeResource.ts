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
    const refresh = () => {
      void queryClient.invalidateQueries({
        queryKey: resourceQueryKeys.all(resourceId),
      });
    };
    const events = [
      `${eventPrefix}.created`,
      `${eventPrefix}.updated`,
      `${eventPrefix}.deleted`,
      "resync",
    ];
    for (const event of events) source.addEventListener(event, refresh);
    return () => source.close();
  }, [enabled, eventPrefix, queryClient, resourceId, resourceSlug]);
}
