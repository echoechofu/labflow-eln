import type { Protocol } from "./domain";

const searchableText = (protocol: Protocol) =>
  [protocol.name, protocol.description, protocol.category]
    .filter(Boolean)
    .join(" ")
    .toLocaleLowerCase();

export const searchProtocols = (protocols: Protocol[], query: string) => {
  const normalizedQuery = query.trim().toLocaleLowerCase();
  if (!normalizedQuery) return [];
  return protocols.filter((protocol) =>
    searchableText(protocol).includes(normalizedQuery),
  );
};
