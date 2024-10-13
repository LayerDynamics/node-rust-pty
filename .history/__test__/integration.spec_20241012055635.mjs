// __test__/integration.spec.mjs
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

// New failure case: Invalid shell path
test("PTY Handle invalid shell path", async (t) => {
	const invalidPtyHandle = new PtyHandle("/invalid-shell-path", []);
	const error = await t.throwsAsync(
		async () => {
			await invalidPtyHandle.write("echo 'This should fail'\n");
		},
		{ instanceOf: Error },
	);

	t.truthy(
		error,
		"Should throw error when initializing with an invalid shell path",
	);
});

// New failure case: Invalid arguments
test("PTY Handle invalid arguments", async (t) => {
	const invalidArgsPtyHandle = new PtyHandle("/bin/bash", ["invalid-argument"]);
	const error = await t.throwsAsync(
		async () => {
			await invalidArgsPtyHandle.write("echo 'This should also fail'\n");
		},
		{ instanceOf: Error },
	);

	t.truthy(
		error,
		"Should throw error when initializing with invalid arguments",
	);
});
