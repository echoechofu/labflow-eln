// Browser-only QA harness: all Tauri calls are in-memory stubs, no user data.
import { useState } from "react";
import { createRoot } from "react-dom/client";
import { ProtocolCreationWizard } from "../src/ProtocolEditor";
import { initialStore } from "../src/repository";
import type { Protocol } from "../src/domain";
import catalog from "../src-tauri/src/protocol_catalog.rs?raw";
import "../src/App.css";
import "../src/task-modal.css";

const store = initialStore();
store.protocols = [...catalog.matchAll(/id: "([^"]+)"[\s\S]*?name: "([^"]+)"[\s\S]*?schema: r#"([\s\S]*?)"#/g)]
  .map((match) => ({
    id: match[1], name: match[2], category: "QA", description: "Isolated capability fixture",
    version: 1, accent: "#6957e8", origin: "builtin", ...JSON.parse(match[3]),
  } as Protocol));
const requests: unknown[] = [];
Object.assign(window, {
  isTauri: true,
  __protocolQaRequests: requests,
  __TAURI_INTERNALS__: {
    invoke: async (command: string, args: unknown) => {
      if (command === "get_store") return store;
      if (command === "save_user_protocol") {
        requests.push(args);
        return { id: "qa-only", version: 1 };
      }
      throw new Error(`Unexpected QA command: ${command}`);
    },
  },
});

export function Fixture() {
  const [open, setOpen] = useState(true);
  return <>
    <p>隔离 UI 验收：仅内存模拟，不保存用户数据。已提交 {requests.length} 次。</p>
    <button onClick={() => setOpen(true)}>重新打开</button>
    {!open && <pre aria-label="提交配置">{JSON.stringify(requests.at(-1), null, 2)}</pre>}
    {open && <ProtocolCreationWizard sampleTypes={store.sampleTypes} close={() => setOpen(false)} saved={() => {}} />}
  </>;
}
createRoot(document.getElementById("root")!).render(<Fixture />);
