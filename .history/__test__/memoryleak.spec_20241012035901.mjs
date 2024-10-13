import { PtyHandle } from "../darwin-x64/index.js"; // Adjust based on your build directory

export function run(test) {
  test("PTY Handle memory leak test", async (t) => {
    const iterations = 1000;
    const initialMemory = process.memoryUsage().heapUsed;

    for (let i = 0; i < iterations; i++) {
      const pty = new PtyHandle('/bin/bash', ['-i']);
      await pty.write('echo "Memory test"\n');
      await pty.read();
      await pty.close();
    }

    // Force garbage collection if possible
    if (global.gc) {
      global.gc();
    }

    const finalMemory = process.memoryUsage().heapUsed;
    const memoryDiff = finalMemory - initialMemory;

    console.log(`Memory usage increased by ${memoryDiff} bytes after ${iterations} iterations`);
    t.true(memoryDiff < 10 * 1024 * 1024, 'Memory increase should be less than 10MB');
  });

  // Add more memory leak tests here...
}