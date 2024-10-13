import test from "ava";
import { PtyHandle } from "../index.js";

test("PtyHandle constructor", async (t) => {
	const ptyHandle = await PtyHandle.new();
	t.true(ptyHandle instanceof PtyHandle, "PtyHandle should be instantiated");
});

test("read operation", async (t) => {
	const ptyHandle = await PtyHandle.new();
	const readResult = await ptyHandle.read();
	t.is(typeof readResult, "string", "Read result should be a string");
});

test("write operation", async (t) => {
	const ptyHandle = await PtyHandle.new();
	await t.notThrowsAsync(
		ptyHandle.write('echo "Hello, World!"'),
		"Write operation should not throw",
	);
});

test("resize operation", async (t) => {
	const ptyHandle = await PtyHandle.new();
	await t.notThrowsAsync(
		ptyHandle.resize(80, 24),
		"Resize operation should not throw",
	);
});

test("close operation", async (t) => {
	const ptyHandle = await PtyHandle.new();
	await t.notThrowsAsync(ptyHandle.close(), "Close operation should not throw");
});

test("read after close", async (t) => {
	const ptyHandle = await PtyHandle.new();
	await ptyHandle.close();
	await t.throwsAsync(
		ptyHandle.read(),
		{ message: /PTY not initialized/ },
		"Read after close should throw",
	);
});

test("write after close", async (t) => {
	const ptyHandle = await PtyHandle.new();
	await ptyHandle.close();
	await t.throwsAsync(
		ptyHandle.write('echo "Hello"'),
		{ message: /Failed to send write command/ },
		"Write after close should throw",
	);
});

test("resize after close", async (t) => {
	const ptyHandle = await PtyHandle.new();
	await ptyHandle.close();
	await t.throwsAsync(
		ptyHandle.resize(100, 30),
		{ message: /Failed to send resize command/ },
		"Resize after close should throw",
	);
});

test("multiple reads", async (t) => {
	const ptyHandle = await PtyHandle.new();
	const readPromises = Array(5)
		.fill()
		.map(() => ptyHandle.read());
	const results = await Promise.all(readPromises);
	results.forEach((result) => {
		t.is(typeof result, "string", "Each read result should be a string");
	});
});

test("write and read", async (t) => {
	const ptyHandle = await PtyHandle.new();
	const testString = 'echo "Test String"';
	await ptyHandle.write(testString);
	const readResult = await ptyHandle.read();
	t.true(
		readResult.includes(testString),
		"Read result should include the written string",
	);
});

test("resize and read", async (t) => {
	const ptyHandle = await PtyHandle.new();
	await ptyHandle.resize(100, 40);
	const readResult = await ptyHandle.read();
	t.is(
		typeof readResult,
		"string",
		"Read result after resize should be a string",
	);
});

test("error handling - invalid resize", async (t) => {
	const ptyHandle = await PtyHandle.new();
	await t.throwsAsync(
		ptyHandle.resize(0, 0),
		{ message: /Failed to resize PTY/ },
		"Resize with invalid dimensions should throw",
	);
});

test("concurrent operations", async (t) => {
	const ptyHandle = await PtyHandle.new();
	const operations = [
		ptyHandle.write('echo "Concurrent 1"'),
		ptyHandle.resize(90, 30),
		ptyHandle.read(),
		ptyHandle.write('echo "Concurrent 2"'),
		ptyHandle.read(),
	];
	await t.notThrowsAsync(
		Promise.all(operations),
		"Concurrent operations should not throw",
	);
});

test("cleanup on drop", async (t) => {
	let ptyHandle = await PtyHandle.new();
	ptyHandle = null;
	await new Promise((resolve) => setTimeout(resolve, 100)); // Allow time for cleanup
	// We can't directly test the cleanup, but we can check that a new PtyHandle can be created
	await t.notThrowsAsync(
		PtyHandle.new(),
		"Should be able to create a new PtyHandle after dropping the previous one",
	);
});
