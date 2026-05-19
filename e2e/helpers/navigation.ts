import { $ } from "@wdio/globals";

/** Navigates to the Assets page and waits for the Add Asset FAB to confirm the route is active. */
export async function navigateToAssets(): Promise<void> {
  const nav = await $('button[aria-label="Assets"]');
  await nav.waitForExist({ timeout: 15000 });
  await nav.click();
  const fab = await $('button[aria-label="Add asset"]');
  await fab.waitForExist({ timeout: 10000 });
}
