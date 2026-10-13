import {
  QueryClient,
  QueryClientProvider,
  useQuery,
} from "@tanstack/react-query";
import { afterEach, describe, expect, it, rs } from "@rstest/core";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { createElement, type ReactNode } from "react";
import { tenantApi, type TenantOrganization } from "./generated/client";
import { appQueryKeys } from "./query";
import { TenantProvider, useTenant } from "./tenant/Tenant";

afterEach(() => {
  cleanup();
  rs.restoreAllMocks();
  tenantApi.clear();
});

describe("tenant selection", () => {
  for (const transition of ["select", "create"] as const) {
    it(`clears observed records on ${transition} and ignores late responses from the previous tenant`, async () => {
      const organization = (id: string): TenantOrganization => ({
        id,
        name: id,
        role: "owner",
        created_at: "2026-10-13T00:00:00Z",
      });
      const alpha = organization("alpha");
      const beta = organization("beta");
      tenantApi.select(alpha.id);
      rs.spyOn(tenantApi, "createOrganization").mockResolvedValue(beta);
      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false, staleTime: Infinity } },
      });
      queryClient.setQueryData(appQueryKeys.tenant.organizations, {
        data: transition === "select" ? [alpha, beta] : [alpha],
      });
      const session = { id: "member" };
      queryClient.setQueryData(appQueryKeys.session, session);
      const key = ["resource", "app::Project", "list"];
      queryClient.setQueryData(key, ["Alpha project"]);
      const pending = new Map<string, (data: string[]) => void>();
      const loadRecords = rs.fn(() => {
        const tenantId = tenantApi.current()!;
        return new Promise<string[]>((resolve) =>
          pending.set(tenantId, resolve),
        );
      });
      const { result } = renderHook(
        () => ({
          tenant: useTenant(),
          records: useQuery({
            queryKey: key,
            queryFn: loadRecords,
            placeholderData: (previous) => previous,
          }),
        }),
        {
          wrapper: ({ children }: { children: ReactNode }) =>
            createElement(
              QueryClientProvider,
              { client: queryClient },
              createElement(TenantProvider, { children }),
            ),
        },
      );
      expect(result.current.records.data).toEqual(["Alpha project"]);
      act(() => {
        void result.current.records.refetch();
      });
      await waitFor(() => expect(pending.has(alpha.id)).toBe(true));
      await act(async () => {
        if (transition === "select") result.current.tenant.select(beta.id);
        else await result.current.tenant.create(beta.name);
      });
      await waitFor(() => expect(pending.has(beta.id)).toBe(true));
      expect(result.current.tenant.current?.id).toBe(beta.id);
      expect(result.current.records.data).toBeUndefined();
      await act(async () => pending.get(alpha.id)!(["Late Alpha project"]));
      expect(result.current.records.data).toBeUndefined();
      await act(async () => pending.get(beta.id)!(["Beta project"]));
      await waitFor(() =>
        expect(result.current.records.data).toEqual(["Beta project"]),
      );
      expect(queryClient.getQueryData(appQueryKeys.session)).toEqual(session);
      queryClient.clear();
    });
  }

  it("continues when browser storage is unavailable", () => {
    const descriptor = Object.getOwnPropertyDescriptor(window, "localStorage");
    Object.defineProperty(window, "localStorage", {
      configurable: true,
      get: () => {
        throw new DOMException("Storage is disabled", "SecurityError");
      },
    });
    try {
      expect(tenantApi.current()).toBeUndefined();
      expect(() => tenantApi.select("tenant-1")).not.toThrow();
      expect(tenantApi.current()).toBe("tenant-1");
      expect(() => tenantApi.clear()).not.toThrow();
      expect(tenantApi.current()).toBeUndefined();
    } finally {
      if (descriptor) Object.defineProperty(window, "localStorage", descriptor);
      else delete (window as { localStorage?: Storage }).localStorage;
    }
  });
});
