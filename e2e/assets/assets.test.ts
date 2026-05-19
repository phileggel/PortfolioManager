/**
 * E2E tests — Asset lifecycle (create, archive, unarchive)
 *
 * Contract: docs/contracts/asset-contract.md
 * Spec rules covered:
 *   AST-001 — create asset → appears in asset table
 *   AST-002 — archive asset → moves to archived state
 *   AST-003 — unarchive asset → returns to active
 */

import assert from "node:assert";
import { $, browser } from "@wdio/globals";
import { dismissLeftoverModal } from "../helpers/modal";
import { navigateToAssets } from "../helpers/navigation";
import { setReactInputValue } from "../helpers/react";
import { seedAsset, seedCategory } from "../helpers/seed";

// ---------------------------------------------------------------------------
// Suite
// ---------------------------------------------------------------------------

describe("assets", () => {
  beforeEach(async () => {
    await dismissLeftoverModal();
    await navigateToAssets();
  });

  // -------------------------------------------------------------------------
  // Create asset → appears in table
  // -------------------------------------------------------------------------
  it("creates an asset manually and it appears in the asset table", async () => {
    const ASSET_NAME = "E2E Asset Create";
    await seedCategory("E2E Cat Create");

    const fab = await $('button[aria-label="Add asset"]');
    await fab.click();

    // WebLookupModal opens first — click Fill manually to go to the form.
    const fillManually = await $('button[aria-label="Fill manually"]');
    await fillManually.waitForDisplayed({ timeout: 8000 });
    await fillManually.click();

    const form = await $("form#add-asset-form");
    await form.waitForExist({ timeout: 8000 });

    await setReactInputValue("add-asset-name", ASSET_NAME);
    await setReactInputValue("add-asset-reference", "TSTCRT");
    await setReactInputValue("add-asset-currency", "EUR");

    // Select category from native <select>.
    const categorySelect = await $("#add-asset-category");
    await categorySelect.waitForExist({ timeout: 5000 });
    await categorySelect.selectByVisibleText("E2E Cat Create");

    // Select class from native <select>.
    const classSelect = await $("#add-asset-class");
    await classSelect.selectByVisibleText("Stocks");

    const submitBtn = await $('button[type="submit"][form="add-asset-form"]');
    await submitBtn.waitForEnabled({ timeout: 5000 });
    await submitBtn.click();

    // After success: handleAddAssetSuccess closes the modal AND calls
    // navigate({to: "/assets"}). Force AssetManager to remount cleanly by
    // navigating away and back — same pattern as buy_sell to defeat any
    // stale loading state caused by the concurrent AssetUpdated event fetch.
    await form.waitForExist({ timeout: 8000, reverse: true });

    const accountsNav = await $('button[aria-label="Accounts"]');
    await accountsNav.waitForExist({ timeout: 10000 });
    await accountsNav.click();
    await $('button[aria-label="Add account"]').waitForExist({
      timeout: 10000,
    });
    await navigateToAssets();

    // Use XPath for reliable text matching — WebdriverIO `*=` selector is flaky in WebKit.
    const assetCell = await $(`//td[normalize-space(text())="${ASSET_NAME}"]`);
    await assetCell.waitForExist({ timeout: 10000 });
    assert.ok(
      await assetCell.isExisting(),
      `Asset "${ASSET_NAME}" must appear in table after creation`,
    );
  });

  // -------------------------------------------------------------------------
  // Archive asset → shows Archived badge; disappears from active list
  // -------------------------------------------------------------------------
  it("archiving an asset removes it from the active list", async () => {
    const ASSET_NAME = "E2E Asset Archive";
    const categoryId = await seedCategory("E2E Cat Archive");
    await seedAsset(ASSET_NAME, categoryId);
    await navigateToAssets();

    // Find and click the Archive icon button scoped to the asset's row,
    // to avoid matching a button in another row when multiple assets are present.
    const archiveBtn = await $(
      `//tr[.//td[contains(., "${ASSET_NAME}")]]//button[@aria-label="Archive"]`,
    );
    await archiveBtn.waitForExist({ timeout: 8000 });
    await archiveBtn.click();

    // Confirm in the dialog (confirmLabel = "Archive") — scoped to dialog.
    const confirmBtn = await $('//*[@role="dialog"]//button[normalize-space()="Archive"]');
    await confirmBtn.waitForDisplayed({ timeout: 5000 });
    await confirmBtn.click();

    // Asset disappears from the active (default) list.
    const assetCell = await $(`*=${ASSET_NAME}`);
    await assetCell.waitForExist({ timeout: 8000, reverse: true });
    assert.ok(
      !(await assetCell.isExisting()),
      `Asset "${ASSET_NAME}" must be hidden from active list after archiving`,
    );
  });

  // -------------------------------------------------------------------------
  // Unarchive asset → returns to active list
  // -------------------------------------------------------------------------
  it("unarchiving an asset returns it to the active list", async () => {
    const ASSET_NAME = "E2E Asset Unarchive";
    const categoryId = await seedCategory("E2E Cat Unarchive");
    const assetId = await seedAsset(ASSET_NAME, categoryId);

    // Archive the asset via IPC so we start in the archived state.
    await browser.executeAsync((id: string, done: (r: unknown) => void) => {
      // @ts-expect-error __TAURI_INTERNALS__ injected by Tauri WebView
      window.__TAURI_INTERNALS__
        .invoke("archive_asset", { id })
        .then(done)
        .catch((err: unknown) => done({ __error: String(err) }));
    }, assetId);

    await navigateToAssets();

    // Show the archived list by checking the "Show archived" checkbox.
    // Click the input directly — label-click propagation is unreliable in WebKitGTK.
    const showArchivedCheckbox = await $(
      '[data-testid="show-archived-toggle"] input[type="checkbox"]',
    );
    await showArchivedCheckbox.waitForExist({ timeout: 5000 });
    await showArchivedCheckbox.click();

    // Use XPath for reliable text matching — WebdriverIO `*=` selector is flaky in WebKit.
    const assetCell = await $(`//td[normalize-space(text())="${ASSET_NAME}"]`);
    await assetCell.waitForExist({ timeout: 10000 });

    // Click Unarchive — scoped to the asset's row.
    const unarchiveBtn = await $(
      `//tr[.//td[contains(., "${ASSET_NAME}")]]//button[@aria-label="Unarchive"]`,
    );
    await unarchiveBtn.waitForExist({ timeout: 5000 });
    await unarchiveBtn.click();

    // Confirm in the dialog (confirmLabel = "Unarchive") — scoped to dialog.
    const confirmBtn = await $('//*[@role="dialog"]//button[normalize-space()="Unarchive"]');
    await confirmBtn.waitForDisplayed({ timeout: 5000 });
    await confirmBtn.click();

    // Wait for the dialog to close (handleUnarchiveConfirm is async — the backdrop
    // would intercept the next click if we toggle showArchived while it's still open).
    const dialog = await $('[role="dialog"]');
    await dialog.waitForExist({ timeout: 8000, reverse: true });

    // Re-query the checkbox after re-render to avoid stale element reference.
    const showArchivedCheckboxAfter = await $(
      '[data-testid="show-archived-toggle"] input[type="checkbox"]',
    );
    await showArchivedCheckboxAfter.waitForExist({ timeout: 5000 });
    await showArchivedCheckboxAfter.click();
    const activeAssetCell = await $(`//td[normalize-space(text())="${ASSET_NAME}"]`);
    await activeAssetCell.waitForExist({ timeout: 8000 });
    assert.ok(
      await activeAssetCell.isExisting(),
      `Asset "${ASSET_NAME}" must appear in active list after unarchiving`,
    );
  });
});
