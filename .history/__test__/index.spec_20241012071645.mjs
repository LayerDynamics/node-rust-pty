import test from "ava";
import { setTimeout } from "timers/promises";
import { PtyHandle } from "../darwin-x64/index.js";
import pino from "pino";

const logger = pino();
const TIMEOUT = 5000; // 5 seconds timeout for operations

// Helper function to create a PtyHandle with timeout
async function createPtyHandle(t) {
	const ptyHandlePromise = PtyHandle.new();
	const timeoutPromise = setTimeout(TIMEOUT, "Timeout");
	const result = await Promise.race([ptyHandlePromise, timeoutPromise]);
	t.not(result, "Timeout", "PtyHandle creation should not timeout");
	return result;
}

test.beforeEach(async (t) => {
	t.context.ptyHandle = await createPtyHandle(t);
});

test.afterEach(async (t) => {
	if (t.context.ptyHandle) {
		await t.context.ptyHandle.close().catch((err) => {
			logger.error("Error closing PtyHandle:", err);
		});
	}
});

test("PtyHandle constructor", async (t) => {
	t.true(
		t.context.ptyHandle instanceof PtyHandle,
		"PtyHandle should be instantiated",
	);
});

test("read operation with timeout", async (t) => {
	const readPromise = t.context.ptyHandle.read();
	const timeoutPromise = setTimeout(TIMEOUT, "Timeout");
	const result = await Promise.race([readPromise, timeoutPromise]);
	t.not(result, "Timeout", "Read operation should not timeout");
	t.is(typeof result, "string", "Read result should be a string");
});

test("write operation", async (t) => {
	await t.notThrowsAsync(async () => {
		const writePromise = t.context.ptyHandle.write('echo "Hello, World!"');
		const timeoutPromise = setTimeout(TIMEOUT, "Timeout");
		const result = await Promise.race([writePromise, timeoutPromise]);
		t.not(result, "Timeout", "Write operation should not timeout");
	}, "Write operation should not throw");
});

test("resize operation", async (t) => {
	await t.notThrowsAsync(async () => {
		const resizePromise = t.context.ptyHandle.resize(80, 24);
		const timeoutPromise = setTimeout(TIMEOUT, "Timeout");
		const result = await Promise.race([resizePromise, timeoutPromise]);
		t.not(result, "Timeout", "Resize operation should not timeout");
	}, "Resize operation should not throw");
});

test("close operation", async (t) => {
	await t.notThrowsAsync(async () => {
		const closePromise = t.context.ptyHandle.close();
		const timeoutPromise = setTimeout(TIMEOUT, "Timeout");
		const result = await Promise.race([closePromise, timeoutPromise]);
		t.not(result, "Timeout", "Close operation should not timeout");
	}, "Close operation should not throw");
	t.context.ptyHandle = null; // Prevent double-closing in afterEach
});

test("operations after close", async (t) => {
	await t.context.ptyHandle.close();
	t.context.ptyHandle = null; // Prevent double-closing in afterEach

	const closedPtyHandle = await createPtyHandle(t);
	await closedPtyHandle.close();

	await t.throwsAsync(
		() => closedPtyHandle.read(),
		{ message: /PTY not initialized/ },
		"Read after close should throw",
	);

	await t.throwsAsync(
		() => closedPtyHandle.write('echo "Hello"'),
		{ message: /Failed to send write command/ },
		"Write after close should throw",
	);

	await t.throwsAsync(
		() => closedPtyHandle.resize(100, 30),
		{ message: /Failed to send resize command/ },
		"Resize after close should throw",
	);
});

test("multiple reads", async (t) => {
	const readPromises = Array(5)
		.fill()
		.map(() => {
			const readPromise = t.context.ptyHandle.read();
			const timeoutPromise = setTimeout(TIMEOUT, "Timeout");
			return Promise.race([readPromise, timeoutPromise]);
		});

	const results = await Promise.all(readPromises);
	results.forEach((result) => {
		t.not(result, "Timeout", "Read operation should not timeout");
		t.is(typeof result, "string", "Each read result should be a string");
	});
});

test("write and read", async (t) => {
	const testString = 'echo "Test String"';
	await t.context.ptyHandle.write(testString);

	const readPromise = t.context.ptyHandle.read();
	const timeoutPromise = setTimeout(TIMEOUT, "Timeout");
	const readResult = await Promise.race([readPromise, timeoutPromise]);

	t.not(readResult, "Timeout", "Read operation should not timeout");
	t.true(
		readResult.includes(testString),
		"Read result should include the written string",
	);
});

test("resize and read", async (t) => {
	await t.context.ptyHandle.resize(100, 40);

	const readPromise = t.context.ptyHandle.read();
	const timeoutPromise = setTimeout(TIMEOUT, "Timeout");
	const readResult = await Promise.race([readPromise, timeoutPromise]);

	t.not(readResult, "Timeout", "Read operation should not timeout");
	t.is(
		typeof readResult,
		"string",
		"Read result after resize should be a string",
	);
});

test("error handling - invalid resize", async (t) => {
	await t.throwsAsync(
		() => t.context.ptyHandle.resize(0, 0),
		{ message: /Failed to resize PTY/ },
		"Resize with invalid dimensions should throw",
	);
});

test("concurrent operations", async (t) => {
	const operations = [
		t.context.ptyHandle.write('echo "Concurrent 1"'),
		t.context.ptyHandle.resize(90, 30),
		t.context.ptyHandle.read(),
		t.context.ptyHandle.write('echo "Concurrent 2"'),
		t.context.ptyHandle.read(),
	];

	await t.notThrowsAsync(async () => {
		const operationsWithTimeout = operations.map((op) => {
			return Promise.race([op, setTimeout(TIMEOUT, "Timeout")]);
		});
		const results = await Promise.all(operationsWithTimeout);
		results.forEach((result) =>
			t.not(result, "Timeout", "Concurrent operation should not timeout"),
		);
	}, "Concurrent operations should not throw");
});

test("cleanup on drop", async (t) => {
	let ptyHandle = await createPtyHandle(t);
	const weakRef = new WeakRef(ptyHandle);
	ptyHandle = null;

	// Force garbage collection if possible
	if (global.gc) {
		global.gc();
	}

	await new Promise((resolve) => setTimeout(resolve, 100)); // Allow time for cleanup

	t.is(weakRef.deref(), undefined, "PtyHandle should be garbage collected");

	// We can't directly test the cleanup, but we can check that a new PtyHandle can be created
	await t.notThrowsAsync(
		createPtyHandle(t),
		"Should be able to create a new PtyHandle after dropping the previous one",
	);
});
