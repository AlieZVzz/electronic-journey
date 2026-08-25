import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

const targets = [
  ["darwin-aarch64", "darwin_aarch64.app.tar.gz"],
  ["darwin-x86_64", "darwin_x64.app.tar.gz"],
  ["windows-x86_64-nsis", "windows_x64.exe"],
  ["windows-x86_64", "windows_x64.exe"],
];

function validate(value, pattern, label) {
  if (!pattern.test(value)) {
    throw new Error(`${label} is invalid.`);
  }
  return value;
}

export async function generateUpdaterManifest({
  version,
  repository,
  signatureDirectory,
}) {
  validate(version, /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/, "version");
  validate(repository, /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/, "repository");

  const prefix = `electronic-journey_${version}_`;
  const platforms = {};
  for (const [target, suffix] of targets) {
    const artifact = `${prefix}${suffix}`;
    const signature = (
      await readFile(join(signatureDirectory, `${artifact}.sig`), "utf8")
    ).trim();
    if (!signature) {
      throw new Error(`signature is empty: ${artifact}.sig`);
    }
    platforms[target] = {
      signature,
      url: `https://github.com/${repository}/releases/download/v${version}/${artifact}`,
    };
  }

  return {
    version,
    notes: `Electronic Journey ${version}。完整更新说明见对应 GitHub Release。`,
    pub_date: new Date().toISOString(),
    platforms,
  };
}

async function selfTest() {
  const directory = await mkdtemp(join(tmpdir(), "electronic-journey-updater-test-"));
  try {
    for (const suffix of new Set(targets.map(([, suffix]) => suffix))) {
      await writeFile(
        join(directory, `electronic-journey_1.2.3_${suffix}.sig`),
        `signature-${suffix}`,
      );
    }
    const manifest = await generateUpdaterManifest({
      version: "1.2.3",
      repository: "owner/repository",
      signatureDirectory: directory,
    });
    assert.equal(manifest.version, "1.2.3");
    assert.equal(Object.keys(manifest.platforms).length, 4);
    assert.equal(
      manifest.platforms["windows-x86_64"].signature,
      manifest.platforms["windows-x86_64-nsis"].signature,
    );
    assert.match(
      manifest.platforms["darwin-aarch64"].url,
      /releases\/download\/v1\.2\.3\/electronic-journey_1\.2\.3_darwin_aarch64\.app\.tar\.gz$/,
    );
    console.log("Updater manifest generator self-test passed.");
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

if (process.argv[2] === "--self-test") {
  await selfTest();
} else if (process.argv[1]?.endsWith("generate-updater-manifest.mjs")) {
  const [version, repository, signatureDirectory, output] = process.argv.slice(2);
  if (!version || !repository || !signatureDirectory || !output) {
    throw new Error(
      "Usage: generate-updater-manifest.mjs <version> <owner/repo> <signature-directory> <output>",
    );
  }
  const manifest = await generateUpdaterManifest({
    version,
    repository,
    signatureDirectory,
  });
  await writeFile(output, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
}
