// __test__/index.spec.mjs
import test from "ava";
import { PtyHandle } from "../index.js"; // Adjust the path to import from index.js

let ptyHandle;

// Run before each test case
test.beforeEach(() => {
	// Create a new PtyHandle with `/bin/bash` as an example shell.
	ptyHandle = new PtyHandle("/bin/bash", ["-l"]);
});

// Cleanup after each test case
test.afterEach(async () => {
	await ptyHandle.close(); // Ensure cleanup after each test.
});

test("PTY Handle initialization", (t) => {
	t.truthy(ptyHandle, "PtyHandle instance should be created.");
	t.truthy(ptyHandle.pty, "PtyHandle should contain valid PTY information.");
});

test("PTY Write Operation", async (t) => {
	const input = 'echo "Hello, PTY!"\n';
	await t.notThrowsAsync(async () => {
		await ptyHandle.write(input); // Write command to PTY.
	}, "Writing to PTY should not throw errors.");
});

test("PTY Read Operation", async (t) => {
	const input = 'echo "Test Read"\n';
	await ptyHandle.write(input); // Write command to PTY.

	const output = await ptyHandle.read(); // Read the output.
	t.true(output.includes("Test Read"), "PTY should return the written output.");
});

test("PTY Resize Operation", async (t) => {
	await t.notThrowsAsync(async () => {
		await ptyHandle.resize(120, 40); // Resize the PTY.
	}, "Resizing PTY should not throw errors.");
});

test("PTY Close Operation", async (t) => {
	await t.notThrowsAsync(async () => {
		await ptyHandle.close(); // Close the PTY.
	}, "Closing PTY should not throw errors.");
});
