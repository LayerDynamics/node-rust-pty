import test from 'ava';
import { PtyHandle } from '../index.js';  // Adjust the path as needed

test('PTY Handle initialization and basic I/O', async (t) => {
  const pty = new PtyHandle('/bin/bash', ['-i']);
  t.truthy(pty, 'PtyHandle instance should be created');

  // Write a command
  await pty.write('echo "Hello, PTY!"\n');

  // Read the output
  const output = await pty.read();
  t.true(output.includes('Hello, PTY!'), 'PTY should return the written output');

  // Close the PTY
  await pty.close();
});

test('PTY Resize operation', async (t) => {
  const pty = new PtyHandle('/bin/bash', ['-i']);

  await t.notThrowsAsync(async () => {
    await pty.resize(120, 40);
  }, 'Resizing PTY should not throw errors');

  // You might want to write a command that outputs the terminal size
  // and check if it matches the new size

  await pty.close();
});

test('PTY Handle multiple write/read operations', async (t) => {
  const pty = new PtyHandle('/bin/bash', ['-i']);

  for (let i = 0; i < 5; i++) {
    await pty.write(`echo "Test ${i}"\n`);
    const output = await pty.read();
    t.true(output.includes(`Test ${i}`), `PTY should return output for Test ${i}`);
  }

  await pty.close();
});

test('PTY Handle error conditions', async (t) => {
  const pty = new PtyHandle('/bin/bash', ['-i']);

  // Test writing after closing
  await pty.close();
  await t.throwsAsync(async () => {
    await pty.write('echo "This should fail"\n');
  }, { instanceOf: Error }, 'Writing to a closed PTY should throw an error');
});