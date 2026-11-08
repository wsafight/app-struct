import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, rs } from "@rstest/core";
import * as clientActual from "./generated/client" with {
  rstest: "importActual",
};
import * as navigationActual from "./navigation" with {
  rstest: "importActual",
};
import { AuthProvider, useAuth } from "./auth/Auth";
import { AdminPagination } from "./auth/AuthPages";
import { Layout } from "./app/Layout";
import type { ReactNode } from "react";

const authMocks = rs.hoisted(() => ({
  me: rs.fn(),
  register: rs.fn(),
  logout: rs.fn(),
  notifications: rs.fn(),
}));

rs.mock("./generated/client", () => ({
  ...clientActual,
  authApi: {
    ...clientActual.authApi,
    me: authMocks.me,
    register: authMocks.register,
    logout: authMocks.logout,
  },
  productApi: {
    ...clientActual.productApi,
    notifications: authMocks.notifications,
  },
}));

rs.mock("./navigation", () => ({
  ...navigationActual,
  NavLink: ({ to, children }: { to: string; children: ReactNode }) => (
    <a href={to}>{children}</a>
  ),
  Outlet: () => null,
  useLocation: () => ({ pathname: "/workspace" }),
}));

__TENANT_MOCK__;

afterEach(cleanup);

describe("AdminPagination", () => {
  it("disables unavailable directions and requests the next page", async () => {
    const onPageChange = rs.fn();
    const user = userEvent.setup();
    render(
      <AdminPagination
        page={1}
        pageSize={25}
        total={60}
        onPageChange={onPageChange}
      />,
    );

    expect(
      (
        screen.getByRole("button", {
          name: "Previous page",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    expect(screen.getByText("Page 1 of 3 (60 total)")).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "Next page" }));
    expect(onPageChange).toHaveBeenCalledWith(2);
  });

  it("treats an empty result as a single page", () => {
    render(
      <AdminPagination
        page={1}
        pageSize={25}
        total={0}
        onPageChange={() => undefined}
      />,
    );

    expect(screen.getByText("Page 1 of 1 (0 total)")).toBeTruthy();
    expect(
      (screen.getByRole("button", { name: "Next page" }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
  });
});

describe("Layout logout", () => {
  it("disables the action while pending and reports failures", async () => {
    const user = {
      id: "user-1",
      email: "member@example.test",
      roles: ["member"],
    };
    let rejectLogout: (reason: Error) => void = () => undefined;
    authMocks.me.mockResolvedValue(user);
    authMocks.notifications.mockResolvedValue({
      data: [],
      total: 0,
      unread: 0,
    });
    authMocks.logout.mockImplementation(
      () =>
        new Promise<void>((_resolve, reject) => {
          rejectLogout = reject;
        }),
    );
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const actor = userEvent.setup();
    render(
      <QueryClientProvider client={queryClient}>
        <AuthProvider>
          <Layout resources={[]} pages={[]} />
        </AuthProvider>
      </QueryClientProvider>,
    );

    expect(await screen.findByText(user.email)).toBeTruthy();
    const button = screen.getByRole("button", { name: "Sign out" });
    await actor.click(button);
    expect((button as HTMLButtonElement).disabled).toBe(true);
    expect(authMocks.logout).toHaveBeenCalledOnce();

    rejectLogout(new Error("Logout failed"));
    expect((await screen.findByRole("alert")).textContent).toContain(
      "Logout failed",
    );
    await waitFor(() =>
      expect((button as HTMLButtonElement).disabled).toBe(false),
    );
  });
});

describe("AuthProvider", () => {
  it("publishes a registered user without dropping the observed session", async () => {
    const user = {
      id: "user-1",
      email: "member@example.test",
      roles: ["member"],
    };
    authMocks.me.mockResolvedValue(null);
    authMocks.register.mockResolvedValue(user);
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    queryClient.setQueryData(["resource", "app::Project"], { stale: true });

    function RegisterHarness() {
      const auth = useAuth();
      return (
        <>
          <span>
            {auth.loading ? "Loading" : (auth.user?.email ?? "Anonymous")}
          </span>
          <button
            type="button"
            onClick={() =>
              void auth.register("member@example.test", "valid-password")
            }
          >
            Register
          </button>
        </>
      );
    }

    const actor = userEvent.setup();
    render(
      <QueryClientProvider client={queryClient}>
        <AuthProvider>
          <RegisterHarness />
        </AuthProvider>
      </QueryClientProvider>,
    );

    expect(await screen.findByText("Anonymous")).toBeTruthy();
    await actor.click(screen.getByRole("button", { name: "Register" }));
    expect(await screen.findByText(user.email)).toBeTruthy();
    expect(
      queryClient.getQueryData(["resource", "app::Project"]),
    ).toBeUndefined();
  });
});
