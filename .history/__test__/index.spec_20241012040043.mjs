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

test("PTY Handle initialization and basic I/O", async (t) => {
	t.truthy(ptyHandle, "PtyHandle instance should be created");

	await ptyHandle.write('echo "Hello, PTY!"\n');
	const output = await ptyHandle.read();
	t.true(
		output.includes("Hello, PTY!"),
		"PTY should return the written output",
	);
});

test("PTY Write Operation", async (t) => {
	const input = 'echo "Hello, PTY!"\n';
	await t.notThrowsAsync(async () => {
		await ptyHandle.write(input);
	}, "Writing to PTY should not throw errors.");
});

test("PTY Read Operation", async (t) => {
	const input = 'echo "Test Read"\n';
	await ptyHandle.write(input);

	const output = await ptyHandle.read();
	t.true(output.includes("Test Read"), "PTY should return the written output.");
});

test("PTY Resize Operation", async (t) => {
	await t.notThrowsAsync(async () => {
		await ptyHandle.resize(120, 40);
	}, "Resizing PTY should not throw errors.");
});

test("PTY Performance test", async (t) => {
	const iterations = 100; // Reduced for quicker testing
	const start = Date.now();

	for (let i = 0; i < iterations; i++) {
		await ptyHandle.write('echo "Performance test"\n');
		await ptyHandle.read();
	}

	const end = Date.now();
	const duration = end - start;

	console.log(`Performed ${iterations} write/read operations in ${duration}ms`);
	t.true(
		duration < 30000,
		"Performance test should complete within 30 seconds",
	);
});

test("PTY Memory usage test", async (t) => {
	const iterations = 100; // Reduced for quicker testing
	const initialMemory = process.memoryUsage().heapUsed;

	for (let i = 0; i < iterations; i++) {
		await ptyHandle.write('echo "Memory test"\n');
		await ptyHandle.read();
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
});
