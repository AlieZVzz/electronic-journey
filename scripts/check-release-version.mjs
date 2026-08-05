import { readFile } from "node:fs/promises";

const packageJson = JSON.parse(await readFile("package.json", "utf8"));
const packageLock = JSON.parse(await readFile("package-lock.json", "utf8"));
const tauriConfig = JSON.parse(
  await readFile("src-tauri/tauri.conf.json", "utf8"),
);
const cargoWorkspace = await readFile("Cargo.toml", "utf8");
const cargoVersion = cargoWorkspace.match(
  /\[workspace\.package\][\s\S]*?\nversion\s*=\s*"([^"]+)"/,
)?.[1];

const versions = new Map([
  ["package.json", packageJson.version],
  ["package-lock.json", packageLock.packages?.[""]?.version],
  ["src-tauri/tauri.conf.json", tauriConfig.version],
  ["Cargo.toml [workspace.package]", cargoVersion],
]);
const expectedVersion = packageJson.version;
const mismatches = [...versions].filter(
  ([, version]) => version !== expectedVersion,
);

if (mismatches.length > 0) {
  const details = [...versions]
    .map(([source, version]) => `${source}: ${version ?? "missing"}`)
    .join("\n");
  throw new Error(`Application versions do not match:\n${details}`);
}

const releaseTag = process.argv[2];
if (releaseTag && releaseTag !== `v${expectedVersion}`) {
  throw new Error(
    `Release tag ${releaseTag} does not match application version v${expectedVersion}.`,
  );
}

console.log(`Application version ${expectedVersion} is consistent.`);
