import init, { NodeClient } from "lumina-node-wasm"

/**
* Spawn a worker running lumina node and get the `NodeClient` connected to it.
*/
export async function spawnNode() {
  await init();
  let worker = new Worker(new URL("worker.js", import.meta.url), { type: "module" });
  let client = await new NodeClient(worker);
  return client;
}

export * from "lumina-node-wasm";
export default init;
