import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const packageJsonPath = resolve(__dirname, "../package.json");
const manifestPath = resolve(__dirname, "../openclaw.plugin.json");

describe("package metadata", () => {
  it("uses a stable package identity and built plugin entrypoint", () => {
    const packageJson = JSON.parse(readFileSync(packageJsonPath, "utf-8"));

    expect(packageJson.name).toBe("@actionbookdev/actionbook");
    expect(packageJson.openclaw.extensions).toEqual(["./dist/index.js"]);
    expect(packageJson.files).toEqual(
      expect.arrayContaining(["dist", "skills", "openclaw.plugin.json"])
    );
    expect(packageJson.files).not.toContain("src/plugin.ts");
    expect(packageJson.files).not.toContain("src/lib/api-client.ts");
  });

  it("declares plugin-managed skills from the package skills directory", () => {
    const manifest = JSON.parse(readFileSync(manifestPath, "utf-8"));

    expect(manifest.id).toBe("actionbook");
    expect(manifest.skills).toEqual(["./skills"]);
    expect(manifest.configSchema.properties.apiUrl.format).toBeUndefined();
  });
});
