import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
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
});
