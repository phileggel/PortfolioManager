import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { NoCashBanner } from "./NoCashBanner";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) => key,
    i18n: { language: "en" },
  }),
}));

describe("NoCashBanner (CSH-095)", () => {
  // CSH-095 — banner exposes the localised message and a "Record a deposit" CTA.
  it("renders the message and the deposit CTA", () => {
    render(<NoCashBanner onRecordDeposit={() => {}} />);
    expect(screen.getByText("cash.no_cash_banner_message")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "cash.no_cash_banner_cta" })).toBeInTheDocument();
  });

  // CSH-095 — clicking the CTA invokes the onRecordDeposit prop (deposit modal opens).
  it("invokes onRecordDeposit when the CTA is clicked", () => {
    const onRecordDeposit = vi.fn();
    render(<NoCashBanner onRecordDeposit={onRecordDeposit} />);
    fireEvent.click(screen.getByRole("button", { name: "cash.no_cash_banner_cta" }));
    expect(onRecordDeposit).toHaveBeenCalledTimes(1);
  });
});
