// __test__/memoryleak.spec.mjs
import test from "ava";
import { PtyHandle } from "../darwin-x64/index.js"; // Adjust based on your build directory

test("PTY Memory usage test", async (t) => {
	const iterations = 100; // Reduced for quicker testing
	const initialMemory = process.memoryUsage().heapUsed;

	for (let i = 0; i < iterations; i++) {
		const ptyHandle = new PtyHandle("/bin/bash", ["-l"]);
		await ptyHandle.write('echo "Memory test"\n');
		await ptyHandle.read();
		await ptyHandle.close();
	}

	// Force garbage collection if possible
	if (global.gc) {
		global.gc();
	}

	const finalMemory = process.memoryUsage().heapUsed;
	const memoryDiff = finalMemory - initialMemory;

	console.log(
		`Memory usage increased by ${memoryDiff} bytes after ${iterations} iterations`,
	);
	t.true(
		memoryDiff < 10 * 1024 * 1024,
		"Memory increase should be less than 10MB",
	);

	// Granular test to check memory at different lifecycle stages
	t.log("Checking memory usage before and after each operation.");
	const beforeEachOperation = process.memoryUsage().heapUsed;
	const handle = new PtyHandle("/bin/bash", ["-l"]);
	const duringOperation = process.memoryUsage().heapUsed;
	await handle.write('echo "Memory check during lifecycle"\n');
	const afterWriteOperation = process.memoryUsage().heapUsed;
	await handle.close();
	const afterCloseOperation = process.memoryUsage().heapUsed;

	t.log(
		`Memory usage - Before: ${beforeEachOperation}, During: ${duringOperation}, After Write: ${afterWriteOperation}, After Close: ${afterCloseOperation}`,
	);
	t.pass();
});
