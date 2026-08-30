export interface RealtimeResourceOptions {
  enabled: boolean;
  resourceId: string;
  resourceSlug: string;
  eventPrefix: string;
}

export function useRealtimeResource(options: RealtimeResourceOptions): void {
  void options;
}
