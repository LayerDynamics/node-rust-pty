import { PtyHandle } from "../darwin-x64/index.js"; // Adjust based on your build directory

export function run(test) {
	let ptyHandle;

	test.beforeEach(() => {
		ptyHandle = new PtyHandle("/bin/bash", ["-l"]);
	});

	test.afterEach(async () => {
		if (ptyHandle) {
			await ptyHandle.close();
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

	// Add more integration tests here...
}
