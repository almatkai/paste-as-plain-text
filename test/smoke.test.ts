import assert from "node:assert/strict";
import test from "node:test";

test("package.json declares Windows+macOS and Tezbar metadata", async () => {
  const pkg = await import("../package.json", { with: { type: "json" } });
  assert.deepEqual(pkg.default.platforms, ["macOS", "Windows"]);
  assert.equal(pkg.default.tezbar.platforms.join(","), "macos,windows");
  assert.match(pkg.default.tezbar.repository, /github\.com\/almatkai\/paste-as-plain-text$/);
});
