export const validProtocolFieldKey = (key: string) =>
  /^[A-Za-z_][A-Za-z0-9_]{0,63}$/.test(key);
