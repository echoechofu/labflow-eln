import assert from "node:assert/strict";
import test from "node:test";
import { normalizeSampleType, sampleTypeLabel } from "../src/domain";

test("sample type comparison uses uppercase canonical values", () => {
  assert.equal(normalizeSampleType("cDNA"), "CDNA");
  assert.equal(normalizeSampleType("cdna"), "CDNA");
  assert.equal(normalizeSampleType("CELL"), "CELL");
});

test("canonical CDNA keeps its scientific UI label", () => {
  assert.equal(sampleTypeLabel("CDNA"), "cDNA");
});
