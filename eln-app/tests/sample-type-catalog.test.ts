import assert from "node:assert/strict";
import test from "node:test";
import { groupSampleTypes, selectableSampleTypes } from "../src/sampleTypeCatalog";

const catalog = [
  { canonicalType: "CELL", displayName: "细胞", origin: "builtin" as const },
  { canonicalType: "RNA", displayName: "RNA", origin: "builtin" as const },
  { canonicalType: "OTHER", displayName: "其他", origin: "builtin" as const },
  { canonicalType: "PLATE", displayName: "孔板", origin: "builtin" as const },
  {
    canonicalType: "BIOFILM",
    displayName: "生物膜",
    origin: "user" as const,
  },
];

test("output type catalog groups materials and removes fallback/container types", () => {
  assert.deepEqual(
    selectableSampleTypes(catalog).map((item) => item.canonicalType),
    ["CELL", "RNA", "BIOFILM"],
  );
  const groups = groupSampleTypes(catalog);
  assert.equal(groups[0].label, "实验对象与培养体系");
  assert.equal(groups.at(-1)?.label, "自定义类型");
  assert.deepEqual(
    groups.at(-1)?.items.map((item) => item.canonicalType),
    ["BIOFILM"],
  );
});
