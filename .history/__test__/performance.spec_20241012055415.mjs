// __test__/performance.spec.mjs
import test from 'ava';
import { PtyHandle } from "../darwin-x64/index.js"; // Adjust based on your build directory

let ptyHandle;

test.beforeEach(async t => {
  ptyHandle = new PtyHandle("/bin/bash", ["-l"]);
});

test.afterEach(async t => {
  if (ptyHandle) {
    try {
      await ptyHandle.write("exit\n");
      await Promise.race([
        ptyHandle.close(),
        new Promise((_, reject) => setTimeout(() => reject(new Error("Close timed out")), 5000))
      ]);
    } catch (error) {
      console.error("Error during PTY close:", error);
    }
  }
});

test("PTY Performance test", async t => {
  const iterations = 100; // Reduced for quicker testing
  const start = Date.now();

  for (let i = 0; i < iterations; i++) {
    await ptyHandle.write('echo "Performance test"\n');
    await ptyHandle.read();
  }

  const end = Date.now();
  const duration = end - start;
  
  console.log(`Performed ${iterations} write/read operations in ${duration}ms`);
  t.true(duration < 30000, 'Performance test should complete within 30 seconds');
});