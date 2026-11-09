export interface ProductProfile {
  display_name: string;
  locale: string;
  timezone: string;
  notifications: boolean;
  mfa_enabled: boolean;
}
export interface ProductNotification {
  id: string;
  tenant_id: string | null;
  title: string;
  body: string;
  target: string;
  read_at: string | null;
  created_at: string;
}
export interface CreditEntry {
  id: string;
  amount: number;
  reason: string;
  expires_at: string | null;
  created_at: string;
}
export interface CreditOverview {
  monthly_allowance: number;
  monthly_remaining: number;
  paid_balance: number;
  available: number;
  resets_at: string;
  data: CreditEntry[];
  page_size: number;
}
export interface TeamMember {
  id: string;
  name: string | null;
  role: string;
  created_at: string;
}
export interface TeamOverview {
  id: string;
  name: string;
  is_owner: boolean;
  seat_limit: number;
  seats_used: number;
  members: TeamMember[];
  members_truncated: boolean;
}
export interface ProductInsights {
  generated_at: string;
  window_days: number;
  visitors: number;
  signups: number;
  activated: number;
  retention_d7: { eligible: number; returned: number };
  revenue_available: boolean;
  revenue: {
    currency: string;
    captured_minor: string;
    refunded_minor: string;
    net_minor: string;
    paying_organizations: number;
  }[];
  sources: {
    source: string;
    medium: string | null;
    visitors: number;
  }[];
  definitions: Record<string, string>;
}
export interface SecurityProof {
  password: string;
  code?: string;
}

function visitAttribution() {
  if (typeof window === "undefined") return {};
  const params = new URLSearchParams(window.location.search);
  const utm = (name: string) => params.get(name) ?? undefined;
  return {
    referrer: document.referrer || undefined,
    utm_source: utm("utm_source"),
    utm_medium: utm("utm_medium"),
    utm_campaign: utm("utm_campaign"),
  };
}
export const productFeatures = {
  team: __TENANT__,
  billing: __BILLING__,
} as const;
export const productApi = {
  profile: (options: RequestOptions = {}) =>
    request<ProductProfile>("/api/auth/settings", options),
  updateProfile: (input: Omit<ProductProfile, "mfa_enabled">) =>
    request<void>("/api/auth/settings", {
      method: "PATCH",
      body: JSON.stringify(input),
    }),
  changePassword: (proof: SecurityProof, newPassword: string) =>
    request<void>("/api/auth/settings/password", {
      method: "POST",
      body: JSON.stringify({ ...proof, new_password: newPassword }),
    }),
  changeEmail: (proof: SecurityProof, email: string) =>
    request<void>("/api/auth/settings/email", {
      method: "POST",
      body: JSON.stringify({ ...proof, email }),
    }),
  confirmEmail: (token: string) =>
    request<void>("/api/auth/settings/email/confirm", {
      method: "POST",
      body: JSON.stringify({ token }),
    }),
  revokeSessions: (proof: SecurityProof) =>
    request<void>("/api/auth/settings/sessions/revoke", {
      method: "POST",
      body: JSON.stringify(proof),
    }),
  setupMfa: (proof: SecurityProof) =>
    request<{ secret: string; uri: string }>("/api/auth/settings/mfa/setup", {
      method: "POST",
      body: JSON.stringify(proof),
    }),
  enableMfa: (code: string) =>
    request<{ recovery_codes: string[]; reauthenticate: boolean }>(
      "/api/auth/settings/mfa/enable",
      { method: "POST", body: JSON.stringify({ code }) },
    ),
  disableMfa: (proof: SecurityProof) =>
    request<void>("/api/auth/settings/mfa/disable", {
      method: "POST",
      body: JSON.stringify(proof),
    }),
  notifications: (page = 1, options: RequestOptions = {}) =>
    request<{ data: ProductNotification[]; total: number; unread: number }>(
      `/api/product/notifications?page=${page}`,
      options,
    ),
  readNotification: (id: string) =>
    request<void>(`/api/product/notifications/${encodeURIComponent(id)}/read`, {
      method: "POST",
    }),
  readAllNotifications: () =>
    request<void>("/api/product/notifications/read", { method: "POST" }),
  credits: (page = 1, options: RequestOptions = {}) =>
    request<CreditOverview>(`/api/product/credits?page=${page}`, options),
  checkoutCredits: (key: string) =>
    request<{ url: string }>("/api/product/credits/checkout", {
      method: "POST",
      headers: { "Idempotency-Key": key },
    }),
  team: (options: RequestOptions = {}) =>
    request<TeamOverview>("/api/product/team", options),
  renameTeam: (name: string) =>
    request<void>("/api/product/team", {
      method: "PATCH",
      body: JSON.stringify({ name }),
    }),
  transferTeam: (memberId: string) =>
    request<void>("/api/product/team/transfer", {
      method: "POST",
      body: JSON.stringify({ member_id: memberId }),
    }),
  removeMember: (id: string) =>
    request<void>(`/api/product/team/members/${encodeURIComponent(id)}`, {
      method: "DELETE",
    }),
  insights: (options: RequestOptions = {}) =>
    request<ProductInsights>("/api/product/insights", options),
  recordVisit: (visitorId: string, page: "/" | "/pricing") =>
    request<void>("/api/product/visits", {
      method: "POST",
      body: JSON.stringify({
        visitor_id: visitorId,
        page,
        consent: true,
        ...visitAttribution(),
      }),
    }),
};
