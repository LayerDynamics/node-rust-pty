import test from "ava";
import { PtyHandle } from "../index.js";

test("PTY Handle performance test", async (t) => {
	const pty = new PtyHandle("/bin/bash", ["-i"]);
	const iterations = 1000;
	const start = Date.now();

	for (let i = 0; i < iterations; i++) {
		await pty.write('echo "Performance test"\n');
		await pty.read();
	}

	const end = Date.now();
	const duration = end - start;

	console.log(`Performed ${iterations} write/read operations in ${duration}ms`);
	t.true(
		duration < 30000,
		"Performance test should complete within 30 seconds",
	);

	await pty.close();
});
