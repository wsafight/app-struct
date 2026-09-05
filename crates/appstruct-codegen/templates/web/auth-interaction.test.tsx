import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AuthProvider, useAuth } from "./auth/Auth";
import { AdminPagination } from "./auth/AuthPages";

const authMocks = vi.hoisted(() => ({
  me: vi.fn(),
  register: vi.fn(),
}));

vi.mock("./generated/client", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./generated/client")>();
  return {
    ...actual,
    authApi: {
      ...actual.authApi,
      me: authMocks.me,
      register: authMocks.register,
    },
  };
});

afterEach(cleanup);

describe("AdminPagination", () => {
  it("disables unavailable directions and requests the next page", async () => {
    const onPageChange = vi.fn();
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
