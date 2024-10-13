// __test__/performance.spec.mjs
import test from "ava";
import { PtyHandle } from "../darwin-x64/index.js"; // Adjust based on your build directory

let ptyHandle;

test.beforeEach(async (t) => {
	ptyHandle = new PtyHandle("/bin/bash", ["-l"]);
});

test.afterEach(async (t) => {
	if (ptyHandle) {
		try {
			await ptyHandle.write("exit\n");
			await Promise.race([
				ptyHandle.close(),
				new Promise((_, reject) =>
					setTimeout(() => reject(new Error("Close timed out")), 5000),
				),
			]);
		} catch (error) {
			console.error("Error during PTY close:", error);
		}
	}
});

test("PTY Performance test", async (t) => {
	const iterations = 100; // Reduced for quicker testing
	const start = Date.now();
	let writeDuration = 0;
	let readDuration = 0;

	for (let i = 0; i < iterations; i++) {
		const writeStart = Date.now();
		await ptyHandle.write('echo "Performance test"\n');
		writeDuration += Date.now() - writeStart;

		const readStart = Date.now();
		await ptyHandle.read();
		readDuration += Date.now() - readStart;
	}

	const totalDuration = Date.now() - start;

	console.log(
		`Performed ${iterations} write/read operations in ${totalDuration}ms`,
	);
	console.log(
		`Average Write Time: ${writeDuration / iterations}ms, Average Read Time: ${readDuration / iterations}ms`,
	);
	t.true(
		totalDuration < 30000,
		"Performance test should complete within 30 seconds",
	);
	t.log(`Write Duration: ${writeDuration}ms, Read Duration: ${readDuration}ms`);
});
