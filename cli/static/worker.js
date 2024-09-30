import init, { NodeWorker } from '/wasm/lumina_node_wasm.js';

Error.stackTraceLimit = 99;

init().then(async () => {
  console.log("self", self);
  const worker = new NodeWorker(self);
  console.log("starting worker: ", worker);

	self.onerror = (msg) => {
		console.log(msg);
	}

  await worker.run();
})
