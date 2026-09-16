import type { ProtocolField } from "./domain";

export function isProtocolFieldVisible(
  field: ProtocolField,
  values: Record<string, string>,
  inputTypes: string[],
) {
  return (
    (!field.visibleWhen ||
      values[field.visibleWhen.key] === field.visibleWhen.value) &&
    (!field.visibleForInputTypes ||
      inputTypes.some((type) => field.visibleForInputTypes?.includes(type)))
  );
}
