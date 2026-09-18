import type { SampleTypeDefinition } from "./domain";

export const NON_MATERIAL_SAMPLE_TYPES = new Set([
  "PLATE",
  "DISH",
  "WELL",
  "OTHER",
]);

const CATEGORY_ORDER = [
  "实验对象与培养体系",
  "组织与体液",
  "细胞组分与培养产物",
  "核酸与文库",
  "蛋白与小分子",
  "自定义类型",
] as const;

const TYPE_CATEGORY: Record<string, (typeof CATEGORY_ORDER)[number]> = {
  ANIMAL: "实验对象与培养体系",
  CELL: "实验对象与培养体系",
  BACTERIA: "实验对象与培养体系",
  FUNGI: "实验对象与培养体系",
  VIRUS: "实验对象与培养体系",
  ORGANOID: "实验对象与培养体系",
  SPHEROID: "实验对象与培养体系",
  TISSUE: "组织与体液",
  WHOLE_BLOOD: "组织与体液",
  SERUM: "组织与体液",
  PLASMA: "组织与体液",
  FECES: "组织与体液",
  NUCLEI: "细胞组分与培养产物",
  SUP: "细胞组分与培养产物",
  FRACTION: "细胞组分与培养产物",
  EV: "细胞组分与培养产物",
  DNA: "核酸与文库",
  RNA: "核酸与文库",
  CDNA: "核酸与文库",
  PLASMID: "核酸与文库",
  AMPLICON: "核酸与文库",
  LIBRARY: "核酸与文库",
  PROTEIN: "蛋白与小分子",
  PEPTIDE: "蛋白与小分子",
  LIPID: "蛋白与小分子",
  METABOLITE: "蛋白与小分子",
};

export interface SampleTypeGroup {
  label: string;
  items: SampleTypeDefinition[];
}

export const selectableSampleTypes = (types: SampleTypeDefinition[]) =>
  types.filter(
    (item) => !NON_MATERIAL_SAMPLE_TYPES.has(item.canonicalType.toUpperCase()),
  );

export const groupSampleTypes = (
  types: SampleTypeDefinition[],
): SampleTypeGroup[] => {
  const groups = new Map<string, SampleTypeDefinition[]>(
    CATEGORY_ORDER.map((label) => [label, []]),
  );
  selectableSampleTypes(types).forEach((item) => {
    const label =
      item.origin === "user"
        ? "自定义类型"
        : TYPE_CATEGORY[item.canonicalType.toUpperCase()] || "自定义类型";
    groups.get(label)?.push(item);
  });
  return CATEGORY_ORDER.map((label) => ({
    label,
    items: groups.get(label) || [],
  })).filter((group) => group.items.length > 0);
};

export const validCanonicalSampleType = (value: string) =>
  /^[A-Z][A-Z0-9_]{0,31}$/.test(value.trim().toUpperCase());
