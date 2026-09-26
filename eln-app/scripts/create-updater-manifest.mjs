import { readFileSync, writeFileSync } from "node:fs";

const required = (name) => {
  const value = process.env[name]?.trim();
  if (!value) throw new Error(`Missing ${name}`);
  return value;
};

const version = required("LABFLOW_RELEASE_VERSION");
const tag = required("LABFLOW_RELEASE_TAG");
const repository = required("LABFLOW_RELEASE_REPOSITORY");
const output = required("LABFLOW_UPDATE_MANIFEST");
const notes = process.env.LABFLOW_RELEASE_NOTES?.trim() || "LabFlow 功能更新与稳定性改进。";
const assetUrl = (fileName) =>
  `https://github.com/${repository}/releases/download/${tag}/${fileName}`;
const signedAsset = (fileVariable, signatureVariable) => {
  const fileName = required(fileVariable);
  const signaturePath = required(signatureVariable);
  return {
    signature: readFileSync(signaturePath, "utf8").trim(),
    url: assetUrl(fileName),
  };
};

const optionalSignedAsset = (fileVariable, signatureVariable) => {
  const fileName = process.env[fileVariable]?.trim();
  const signaturePath = process.env[signatureVariable]?.trim();
  if (!fileName && !signaturePath) return undefined;
  if (!fileName || !signaturePath) {
    throw new Error(`${fileVariable} and ${signatureVariable} must be set together`);
  }
  return {
    signature: readFileSync(signaturePath, "utf8").trim(),
    url: assetUrl(fileName),
  };
};

const windowsAsset = optionalSignedAsset(
  "LABFLOW_WINDOWS_UPDATE_FILE",
  "LABFLOW_WINDOWS_SIGNATURE",
);

const manifest = {
  version,
  notes,
  pub_date: new Date().toISOString(),
  platforms: {
    "darwin-aarch64": signedAsset(
      "LABFLOW_DARWIN_UPDATE_FILE",
      "LABFLOW_DARWIN_SIGNATURE",
    ),
    ...(windowsAsset ? { "windows-x86_64": windowsAsset } : {}),
  },
};

writeFileSync(output, `${JSON.stringify(manifest, null, 2)}\n`);
