import test from "ava";

// Import all test files
import "./integration.spec.mjs";
import "./performance.spec.mjs";
import "./memoryleak.spec.mjs";

// You can add any additional setup or teardown here if needed
test.before((t) => {
	console.log("Starting PTY test suite...");
});

test.after((t) => {
	console.log("PTY test suite completed.");
});

// Global test case for the suite run
test("Dummy test to ensure test suite runs", (t) => {
	t.pass();
});
