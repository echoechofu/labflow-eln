import assert from "node:assert/strict";
import test from "node:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { ProtocolFields } from "../src/ProtocolFields.tsx";
import { validProtocolFieldKey } from "../src/protocolFieldKey.ts";
import { isProtocolFieldVisible } from "../src/protocolFieldVisibility.ts";
import type { Protocol, ProtocolField } from "../src/domain.ts";

test("Protocol Record field keys match the backend placeholder rules", () => {
  assert.equal(validProtocolFieldKey("dose"), true);
  assert.equal(validProtocolFieldKey("treatment_dose_2"), true);
  assert.equal(validProtocolFieldKey("_internal"), true);
  assert.equal(validProtocolFieldKey("2dose"), false);
  assert.equal(validProtocolFieldKey("刺激浓度"), false);
  assert.equal(validProtocolFieldKey("has-dash"), false);
  assert.equal(validProtocolFieldKey("a".repeat(64)), true);
  assert.equal(validProtocolFieldKey("a".repeat(65)), false);
});

test("conditional Record fields require both value and input-type visibility", () => {
  const field: ProtocolField = {
    key: "dose",
    label: "Dose",
    kind: "number",
    required: true,
    visibleWhen: { key: "mode", value: "yes" },
    visibleForInputTypes: ["CELL"],
  };
  assert.equal(isProtocolFieldVisible(field, { mode: "yes" }, ["CELL"]), true);
  assert.equal(isProtocolFieldVisible(field, { mode: "no" }, ["CELL"]), false);
  assert.equal(
    isProtocolFieldVisible(field, { mode: "yes" }, ["PLATE"]),
    false,
  );
  assert.equal(isProtocolFieldVisible(field, {}, []), false);
});

test("Record field rendering preserves decimal constraints and eligible Samples", () => {
  const protocol = { execution: { inputTypes: ["RNA"] } } as Protocol;
  const common = {
    protocol,
    taskExperimentId: "exp",
    values: {},
    inputTypes: ["RNA"],
    plateCapacity: 0,
    conditionPlateCapacity: 0,
    plateMapping: false,
    plateGroups: [],
    conditionGroups: [],
    setValue: () => {},
    setPlateGroups: () => {},
    setConditionGroups: () => {},
    samples: [
      { id: "valid", code: "VALID", type: "rna", experimentId: "exp" },
      {
        id: "used",
        code: "CONSUMED",
        type: "RNA",
        experimentId: "exp",
        consumed: true,
      },
      { id: "foreign", code: "FOREIGN", type: "RNA", experimentId: "other" },
    ],
  };
  const html = renderToStaticMarkup(
    createElement(ProtocolFields, {
      ...common,
      field: {
        key: "dose",
        label: "Dose",
        kind: "number",
        unit: "µg",
        min: 0,
        max: 10,
      },
    }),
  );
  assert.match(html, /type="number"/);
  assert.match(html, /step="any"/);
  assert.match(html, /min="0"/);
  assert.match(html, /max="10"/);
  assert.match(html, /µg/);
  const samples = renderToStaticMarkup(
    createElement(ProtocolFields, {
      ...common,
      field: { key: "input_sample", label: "Input", kind: "samples" },
    }),
  );
  assert.match(samples, /VALID/);
  assert.doesNotMatch(samples, /CONSUMED|FOREIGN/);
});
