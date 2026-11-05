export interface BusinessMetric {
  key: string;
  value: number | null;
  definition: string;
  source: string;
  window: string;
  unavailable_reason: string | null;
}
export interface BusinessOverview {
  generated_at: string;
  metrics: BusinessMetric[];
}
export const businessApi = {
  overview: (options: RequestOptions = {}) =>
    request<BusinessOverview>("/api/admin/business", options),
};
