// __test__/index.spec.mjs
import test from "ava";
import { setTimeout } from "timers/promises";
import { PtyHandle } from "../darwin-x64/index.js";
import pino from "pino";

const logger = pino();
const TIMEOUT = 10000; // 10 seconds timeout for operations

let ptyHandleCounter = 0;

function incrementPtyHandleCounter() {
	ptyHandleCounter++;
	console.log(`PtyHandle instances created: ${ptyHandleCounter}`);
}

function decrementPtyHandleCounter() {
	ptyHandleCounter--;
	console.log(`PtyHandle instances remaining: ${ptyHandleCounter}`);
}

async function createPtyHandle(t) {
	try {
		console.log(`Creating PtyHandle for test: ${t.title}`);
		const ptyHandlePromise = PtyHandle.new();
		const timeoutPromise = setTimeout(TIMEOUT, "Timeout");
		const result = await Promise.race([ptyHandlePromise, timeoutPromise]);
		if (result === "Timeout") {
			throw new Error("PtyHandle creation timed out");
		}
		incrementPtyHandleCounter();
		return result;
	} catch (error) {
		logger.error(`Error creating PtyHandle for test ${t.title}:`, error);
		t.fail(`Failed to create PtyHandle: ${error.message}`);
	}
}

test.beforeEach(async (t) => {
	t.timeout(TIMEOUT + 1000); // Set AVA's per-test timeout
	console.log(`Starting test: ${t.title}`);
	try {
		t.context.ptyHandle = await createPtyHandle(t);
	} catch (error) {
		console.log(
			`Failed to create PtyHandle in beforeEach for test: ${t.title}`,
		);
	}
});

test.afterEach(async (t) => {
	console.log(`Cleaning up after test: ${t.title}`);
	if (t.context.ptyHandle) {
		try {
			await t.context.ptyHandle.close();
			decrementPtyHandleCounter();
		} catch (error) {
			logger.error(`Error closing PtyHandle for test ${t.title}:`, error);
		}
	}
	console.log(`Finished test: ${t.title}`);
});

test("PtyHandle constructor", async (t) => {
	const ptyHandle = await createPtyHandle(t);
	t.truthy(ptyHandle, "PtyHandle should be created");
});

test("read operation with timeout", async (t) => {
	if (!t.context.ptyHandle) {
		t.fail("PtyHandle not available");
		return;
	}

	try {
		console.log("Starting read operation");
		const readPromise = t.context.ptyHandle.read();
		const timeoutPromise = setTimeout(TIMEOUT, "Timeout");
		const result = await Promise.race([readPromise, timeoutPromise]);
		if (result === "Timeout") {
			t.fail("Read operation timed out");
		} else {
			t.pass("Read operation completed within timeout");
			t.is(typeof result, "string", "Read result should be a string");
		}
		console.log("Finished read operation");
	} catch (error) {
		t.fail(`Read operation failed: ${error.message}`);
	}
});

test("write operation", async (t) => {
	if (!t.context.ptyHandle) {
		t.fail("PtyHandle not available");
		return;
	}

	try {
		console.log("Starting write operation");
		const writePromise = t.context.ptyHandle.write('echo "Hello, World!"');
		const timeoutPromise = setTimeout(TIMEOUT, "Timeout");
		const result = await Promise.race([writePromise, timeoutPromise]);
		if (result === "Timeout") {
			t.fail("Write operation timed out");
		} else {
			t.pass("Write operation completed within timeout");
		}
		console.log("Finished write operation");
	} catch (error) {
		t.fail(`Write operation failed: ${error.message}`);
	}
});

test("resize operation", async (t) => {
	if (!t.context.ptyHandle) {
		t.fail("PtyHandle not available");
		return;
	}

	try {
		console.log("Starting resize operation");
		const resizePromise = t.context.ptyHandle.resize(80, 24);
		const timeoutPromise = setTimeout(TIMEOUT, "Timeout");
		const result = await Promise.race([resizePromise, timeoutPromise]);
		if (result === "Timeout") {
			t.fail("Resize operation timed out");
		} else {
			t.pass("Resize operation completed within timeout");
		}
		console.log("Finished resize operation");
	} catch (error) {
		t.fail(`Resize operation failed: ${error.message}`);
	}
});

test("close operation", async (t) => {
	if (!t.context.ptyHandle) {
		t.fail("PtyHandle not available");
		return;
	}

	try {
		console.log("Starting close operation");
		const closePromise = t.context.ptyHandle.close();
		const timeoutPromise = setTimeout(TIMEOUT, "Timeout");
		const result = await Promise.race([closePromise, timeoutPromise]);
		if (result === "Timeout") {
			t.fail("Close operation timed out");
		} else {
			t.pass("Close operation completed within timeout");
		}
		t.context.ptyHandle = null; // Prevent double-closing in afterEach
		decrementPtyHandleCounter();
		console.log("Finished close operation");
	} catch (error) {
		t.fail(`Close operation failed: ${error.message}`);
	}
});

test("operations after close", async (t) => {
	if (!t.context.ptyHandle) {
		t.fail("PtyHandle not available");
		return;
	}

	try {
		console.log("Starting operations after close test");
		await t.context.ptyHandle.close();
		t.context.ptyHandle = null; // Prevent double-closing in afterEach
		decrementPtyHandleCounter();

		const closedPtyHandle = await createPtyHandle(t);
		if (!closedPtyHandle) {
			t.fail("Failed to create new PtyHandle for closed operations test");
			return;
		}

		await closedPtyHandle.close();
		decrementPtyHandleCounter();

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
		console.log("Finished operations after close test");
	} catch (error) {
		t.fail(`Operations after close test failed: ${error.message}`);
	}
});

test("multiple reads", async (t) => {
	if (!t.context.ptyHandle) {
		t.fail("PtyHandle not available");
		return;
	}

	try {
		console.log("Starting multiple reads test");
		const readPromises = Array(5)
			.fill()
			.map((_, index) => {
				console.log(`Starting read operation ${index + 1}`);
				const readPromise = t.context.ptyHandle.read();
				const timeoutPromise = setTimeout(TIMEOUT, "Timeout");
				return Promise.race([readPromise, timeoutPromise]);
			});

		const results = await Promise.all(readPromises);
		results.forEach((result, index) => {
			if (result === "Timeout") {
				t.fail(`Read operation ${index + 1} timed out`);
			} else {
				t.is(
					typeof result,
					"string",
					`Read result ${index + 1} should be a string`,
				);
			}
			console.log(`Finished read operation ${index + 1}`);
		});
		console.log("Finished multiple reads test");
	} catch (error) {
		t.fail(`Multiple reads test failed: ${error.message}`);
	}
});

test("write and read", async (t) => {
	if (!t.context.ptyHandle) {
		t.fail("PtyHandle not available");
		return;
	}

	try {
		console.log("Starting write and read test");
		const testString = 'echo "Test String"';
		await t.context.ptyHandle.write(testString);

		const readPromise = t.context.ptyHandle.read();
		const timeoutPromise = setTimeout(TIMEOUT, "Timeout");
		const readResult = await Promise.race([readPromise, timeoutPromise]);

		if (readResult === "Timeout") {
			t.fail("Read operation timed out");
		} else {
			t.true(
				readResult.includes(testString),
				"Read result should include the written string",
			);
		}
		console.log("Finished write and read test");
	} catch (error) {
		t.fail(`Write and read test failed: ${error.message}`);
	}
});

test("resize and read", async (t) => {
	if (!t.context.ptyHandle) {
		t.fail("PtyHandle not available");
		return;
	}

	try {
		console.log("Starting resize and read test");
		await t.context.ptyHandle.resize(100, 40);

		const readPromise = t.context.ptyHandle.read();
		const timeoutPromise = setTimeout(TIMEOUT, "Timeout");
		const readResult = await Promise.race([readPromise, timeoutPromise]);

		if (readResult === "Timeout") {
			t.fail("Read operation after resize timed out");
		} else {
			t.is(
				typeof readResult,
				"string",
				"Read result after resize should be a string",
			);
		}
		console.log("Finished resize and read test");
	} catch (error) {
		t.fail(`Resize and read test failed: ${error.message}`);
	}
});

test("error handling - invalid resize", async (t) => {
	if (!t.context.ptyHandle) {
		t.fail("PtyHandle not available");
		return;
	}

	try {
		console.log("Starting invalid resize test");
		await t.throwsAsync(
			() => t.context.ptyHandle.resize(0, 0),
			{ message: /Failed to resize PTY/ },
			"Resize with invalid dimensions should throw",
		);
		console.log("Finished invalid resize test");
	} catch (error) {
		t.fail(`Invalid resize test failed: ${error.message}`);
	}
});

test("concurrent operations", async (t) => {
	if (!t.context.ptyHandle) {
		t.fail("PtyHandle not available");
		return;
	}

	try {
		console.log("Starting concurrent operations test");
		const operations = [
			t.context.ptyHandle.write('echo "Concurrent 1"'),
			t.context.ptyHandle.resize(90, 30),
			t.context.ptyHandle.read(),
			t.context.ptyHandle.write('echo "Concurrent 2"'),
			t.context.ptyHandle.read(),
		];

		const operationsWithTimeout = operations.map((op, index) => {
			console.log(`Starting concurrent operation ${index + 1}`);
			return Promise.race([op, setTimeout(TIMEOUT, "Timeout")]);
		});

		const results = await Promise.all(operationsWithTimeout);
		results.forEach((result, index) => {
			if (result === "Timeout") {
				t.fail(`Concurrent operation ${index + 1} timed out`);
			} else {
				t.pass(`Concurrent operation ${index + 1} completed within timeout`);
			}
			console.log(`Finished concurrent operation ${index + 1}`);
		});
		console.log("Finished concurrent operations test");
	} catch (error) {
		t.fail(`Concurrent operations test failed: ${error.message}`);
	}
});

test("cleanup on drop", async (t) => {
	console.log("Starting cleanup on drop test");
	let ptyHandle = await createPtyHandle(t);
	if (!ptyHandle) {
		t.fail("Failed to create PtyHandle for cleanup test");
		return;
	}

	const weakRef = new WeakRef(ptyHandle);
	ptyHandle = null;

	// Force garbage collection if possible
	if (global.gc) {
		console.log("Forcing garbage collection");
		global.gc();
	}

	await new Promise((resolve) => setTimeout(resolve, 100)); // Allow time for cleanup

	t.is(weakRef.deref(), undefined, "PtyHandle should be garbage collected");

	try {
		const newPtyHandle = await createPtyHandle(t);
		t.truthy(
			newPtyHandle,
			"Should be able to create a new PtyHandle after dropping the previous one",
		);
		await newPtyHandle.close();
		decrementPtyHandleCounter();
	} catch (error) {
		t.fail(`Failed to create new PtyHandle after cleanup: ${error.message}`);
	}
	console.log("Finished cleanup on drop test");
});

// Log the final count of PtyHandle instances
test.after.always(() => {
	console.log(`Final PtyHandle instance count: ${ptyHandleCounter}`);
});
