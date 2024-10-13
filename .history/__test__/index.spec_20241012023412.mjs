// __test__/index.spec.mjs
import test from "ava";
import { PtyHandle } from "../darwin-x64/index.js"; // Adjust based on your build directory
const { Terminal } = require("xterm");



let ptyHandle;
let terminal;

// Run before each test case
test.beforeEach(() => {
	// Create a new PtyHandle with `/bin/bash` as an example shell.
	ptyHandle = new PtyHandle("/bin/bash", ["-l"]);

	// Initialize a new xterm.js terminal instance
	terminal = new Terminal();
	terminal.open(document.getElementById("terminal-container")); // You need a container for xterm in your DOM for full interactivity
});

// Cleanup after each test case
test.afterEach(async () => {
	await ptyHandle.close(); // Ensure cleanup after each test.
	terminal.dispose(); // Cleanup xterm.js terminal instance
});

test("PTY Handle initialization", async (t) => {
	t.truthy(ptyHandle, "PtyHandle instance should be created.");
	// Since 'pty' is not exposed, we'll check if ptyHandle is operational by performing a read or write

	const input = 'echo "Hello, PTY!"\n';
	await ptyHandle.write(input);

	// Connect terminal data to the PtyHandle read
	ptyHandle.on("data", (data) => {
		terminal.write(data); // Write PTY data to terminal
	});

	const output = await ptyHandle.read();
	t.true(
		output.includes("Hello, PTY!"),
		"PTY should return the written output.",
	);
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

	// Use xterm.js to visualize the read output
	ptyHandle.on("data", (data) => {
		terminal.write(data); // Write PTY data to terminal
	});

	const output = await ptyHandle.read(); // Read the output.
	t.true(output.includes("Test Read"), "PTY should return the written output.");
});

test("PTY Resize Operation", async (t) => {
	await t.notThrowsAsync(async () => {
		await ptyHandle.resize(120, 40); // Resize the PTY.
		// Optionally resize the terminal to simulate visual changes
		terminal.resize(120, 40);
	}, "Resizing PTY should not throw errors.");
});

test("PTY Close Operation", async (t) => {
	await t.notThrowsAsync(async () => {
		await ptyHandle.close(); // Close the PTY.
	}, "Closing PTY should not throw errors.");
});
