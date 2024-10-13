import test from "ava";
import { PtyHandle } from "../darwin-x64/index.js"; // Adjust based on your build directory

// Manually mock xterm.js for the tests
class MockTerminal {
  open() {}
  write() {}
  dispose() {}
  resize() {}
}

let ptyHandle;
let terminal;

// Run before each test case
test.beforeEach(() => {
	// Create a new PtyHandle with `/bin/bash` as an example shell.
	ptyHandle = new PtyHandle("/bin/bash", ["-l"]);

	// Use the manually mocked terminal instead of xterm.js
	terminal = new MockTerminal();
	terminal.open(); // Mocked terminal open
});

// Forcefully close the PtyHandle process
async function forceClosePty(ptyHandle) {
	try {
		if (ptyHandle) {
			await ptyHandle.write('exit\n'); // Try to gracefully close the process

			// If the process still doesn't close, kill it after a delay
			const timeout = new Promise((resolve) => setTimeout(resolve, 5000));
			await Promise.race([ptyHandle.close(), timeout]);

			if (timeout) {
				console.warn('Forcefully killing the PTY process');
				// If close doesn't resolve, kill the process
				ptyHandle.kill();
			}
		}
	} catch (error) {
		console.error("Error during PTY close operation:", error);
	}
}

// Cleanup after each test case
test.afterEach(async () => {
	await forceClosePty(ptyHandle); // Ensure cleanup after each test.
	terminal.dispose(); // Cleanup mocked xterm.js terminal instance
});

test("PTY Handle initialization", async (t) => {
	t.truthy(ptyHandle, "PtyHandle instance should be created.");
	// Since 'pty' is not exposed, we'll check if ptyHandle is operational by performing a read or write

	const input = 'echo "Hello, PTY!"\n';
	await ptyHandle.write(input);

	// Simulate terminal data
	ptyHandle.on("data", (data) => {
		terminal.write(data); // Write PTY data to terminal (mocked)
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

	// Simulate terminal output using mocked terminal
	ptyHandle.on("data", (data) => {
		terminal.write(data); // Write PTY data to terminal (mocked)
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
		// Attempt to close the PTY and catch potential timeouts
		await ptyHandle.close(); // Close the PTY.
	}, "Closing PTY should not throw errors.");
});
