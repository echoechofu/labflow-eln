import assert from "node:assert/strict";
import test from "node:test";
import type { Protocol } from "../src/domain.ts";
import { searchProtocols } from "../src/protocolSearch.ts";

const protocol = (
  id: string,
  name: string,
  category: string,
  description = "",
): Protocol => ({
  id,
  name,
  category,
  description,
  version: 1,
  blocks: [],
  accent: "#6957e8",
});

const protocols = [
  protocol("rna", "RNA Extraction — Trizol", "分子生物学", "提取 RNA"),
  protocol("passage", "细胞传代", "细胞培养", "细胞培养与扩增"),
  protocol("qpcr", "SYBR Green qPCR", "分子生物学", "检测基因表达"),
];

test("empty Protocol search does not display the full list", () => {
  assert.deepEqual(searchProtocols(protocols, "   "), []);
});

test("Protocol search matches name, description, and category", () => {
  assert.deepEqual(
    searchProtocols(protocols, "QPCR").map((item) => item.id),
    ["qpcr"],
  );
  assert.deepEqual(
    searchProtocols(protocols, "扩增").map((item) => item.id),
    ["passage"],
  );
  assert.deepEqual(
    searchProtocols(protocols, "分子生物学").map((item) => item.id),
    ["rna", "qpcr"],
  );
});
