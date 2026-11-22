import { useState } from "react";
import { expect, it } from "vitest";
import userEvent from "@testing-library/user-event";
import { renderWithProviders, screen } from "@test/helpers/render";
import ClearSearchButton from "@/components/search/ClearSearchButton";
function SearchBox({ floating = false }: { floating?: boolean }) {
  const [value, setValue] = useState("hello");
  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        throw new Error("clear must not submit");
      }}
    >
      <div className="relative">
        <input aria-label="Query" value={value} onChange={(e) => setValue(e.target.value)} />
        {value && <ClearSearchButton floating={floating} onClear={() => setValue("")} />}
      </div>
    </form>
  );
}
it.each([false, true])(
  "clears input, hides itself, and returns focus (floating=%s)",
  async (floating) => {
    renderWithProviders(<SearchBox floating={floating} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Clear input" }));
    expect(screen.getByRole("textbox", { name: "Query" })).toHaveValue("");
    expect(screen.getByRole("textbox")).toHaveFocus();
    expect(screen.queryByRole("button", { name: "Clear input" })).not.toBeInTheDocument();
    await user.type(screen.getByRole("textbox"), "new");
    expect(screen.getByRole("button", { name: "Clear input" })).toBeVisible();
  },
);
it("supports clearing from the keyboard without submitting the form", async () => {
  renderWithProviders(<SearchBox />);
  const user = userEvent.setup();
  screen.getByRole("button", { name: "Clear input" }).focus();
  await user.keyboard("{Enter}");
  expect(screen.getByRole("textbox")).toHaveValue("");
  expect(screen.getByRole("textbox")).toHaveFocus();
});
