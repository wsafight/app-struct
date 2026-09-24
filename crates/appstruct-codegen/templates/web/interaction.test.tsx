import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AsyncState } from "./components/AsyncState";
import { ConfirmDialog } from "./components/Dialog";

afterEach(cleanup);

describe("ConfirmDialog", () => {
  it("requires an explicit confirmation", async () => {
    const onConfirm = vi.fn();
    const user = userEvent.setup();
    render(
      <ConfirmDialog
        open
        title="Delete record"
        description="This cannot be undone."
        confirmLabel="Delete"
        danger
        onCancel={() => undefined}
        onConfirm={onConfirm}
      />,
    );

    expect(screen.getByRole("dialog", { name: "Delete record" })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Delete" }));
    expect(onConfirm).toHaveBeenCalledOnce();
  });

  it("moves focus into an open dialog and restores it after close", async () => {
    const trigger = document.createElement("button");
    document.body.append(trigger);
    trigger.focus();
    const view = render(
      <ConfirmDialog
        open
        title="Delete record"
        description="This cannot be undone."
        onCancel={() => undefined}
        onConfirm={() => undefined}
      />,
    );

    await waitFor(() =>
      expect(document.activeElement).toBe(
        screen.getByRole("button", { name: "Cancel" }),
      ),
    );
    view.rerender(
      <ConfirmDialog
        open={false}
        title="Delete record"
        description="This cannot be undone."
        onCancel={() => undefined}
        onConfirm={() => undefined}
      />,
    );
    await waitFor(() => expect(document.activeElement).toBe(trigger));
    trigger.remove();
  });

  it("exposes a retry action for async errors", async () => {
    const onRetry = vi.fn();
    const user = userEvent.setup();
    render(
      <AsyncState
        state="error"
        message="Could not load records"
        onRetry={onRetry}
      />,
    );

    expect(screen.getByRole("alert").textContent).toContain(
      "Could not load records",
    );
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(onRetry).toHaveBeenCalledOnce();
  });
});
