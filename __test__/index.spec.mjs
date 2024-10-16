import test from 'ava';
import { setTimeout } from 'timers/promises';
import pino from 'pino';

// Dynamic import based on platform
const platform = process.platform;
const arch = process.arch;
let nativeBinding;

try {
	switch (platform) {
		case 'darwin':
			nativeBinding = await import(`../npm/prebuilds/darwin-${arch}/index.js`);
			break;
		case 'linux':
			nativeBinding = await import(
				`../npm/prebuilds/linux-${arch}-gnu/index.js`
			);
			break;
		case 'win32':
			nativeBinding = await import(
				`../npm/prebuilds/win32-${arch}-msvc/index.js`
			);
			break;
		default:
			throw new Error(`Unsupported platform: ${platform}`);
	}
} catch (error) {
	console.error(`Failed to import native binding: ${error}`);
	process.exit(1);
}

const { PtyHandle, MultiplexerHandle } = nativeBinding;

console.log('PtyHandle Imported:', PtyHandle);
console.log('MultiplexerHandle Imported:', MultiplexerHandle);

// Initialize Logger
const logger = pino();
const TIMEOUT = 30000; // Increased timeout to 30 seconds

let ptyHandleCounter = 0;

function incrementPtyHandleCounter() {
	ptyHandleCounter++;
	console.log(`PtyHandle instances created: ${ptyHandleCounter}`);
}

function decrementPtyHandleCounter() {
	ptyHandleCounter--;
	console.log(`PtyHandle instances remaining: ${ptyHandleCounter}`);
}

async function createPtyHandle(t) {
	try {
		console.log(`Creating PtyHandle for test: ${t.title}`);
		const ptyHandlePromise = PtyHandle.new();
		const timeoutPromise = setTimeout(TIMEOUT, 'Timeout');
		const result = await Promise.race([ptyHandlePromise, timeoutPromise]);

		if (result === 'Timeout') {
			throw new Error('PtyHandle creation timed out');
		}

		incrementPtyHandleCounter();
		return result;
	} catch (error) {
		logger.error(`Error creating PtyHandle for test ${t.title}:`, error);
		t.fail(`Failed to create PtyHandle: ${error.message}`);
		throw error;
	}
}

test.beforeEach(async (t) => {
	t.timeout(TIMEOUT + 1000);
	console.log(`Starting test: ${t.title}`);
	try {
		t.context.ptyHandle = await createPtyHandle(t);
	} catch (error) {
		console.log(
			`Failed to create PtyHandle in beforeEach for test: ${t.title}`,
		);
	}
});

test.afterEach(async (t) => {
	console.log(`Cleaning up after test: ${t.title}`);
	if (t.context.ptyHandle) {
		try {
			await t.context.ptyHandle.close();
			decrementPtyHandleCounter();
		} catch (error) {
			logger.error(`Error closing PtyHandle for test ${t.title}:`, error);
		}
	}
	console.log(`Finished test: ${t.title}`);
});

test('PtyHandle constructor', async (t) => {
	const ptyHandle = await createPtyHandle(t);
	t.truthy(ptyHandle, 'PtyHandle should be created');
});

test('MultiplexerHandle operations', async (t) => {
	if (!t.context.ptyHandle) {
		t.fail('PtyHandle not available');
		return;
	}

	try {
		console.log('Starting MultiplexerHandle operation');
		const multiplexerHandle = t.context.ptyHandle.multiplexerHandle;
		t.truthy(multiplexerHandle, 'MultiplexerHandle should be available');

		// Create sessions with proper delay
		const session1 = await multiplexerHandle.createSession();
		await setTimeout(500); // Ensure time for session creation
		const session2 = await multiplexerHandle.createSession();
		await setTimeout(500);
		t.truthy(session1, 'Session 1 should be created');
		t.truthy(session2, 'Session 2 should be created');
		console.log('Created sessions:', session1, session2);

		// Send data to session 1 and read from it
		console.log('Sending data to session 1...');
		await multiplexerHandle.sendToSession(
			session1,
			Array.from(Buffer.from('test data 1')),
		);
		await setTimeout(500); // Ensure time for data to process
		console.log('Reading data from session 1...');
		const output1 = await multiplexerHandle.readFromSession(session1);
		console.log('Output from session 1:', output1);
		t.is(
			output1.trim(),
			'test data 1',
			'Session 1 should receive the correct output',
		);

		// Send data to session 2 and read from it
		console.log('Sending data to session 2...');
		await multiplexerHandle.sendToSession(
			session2,
			Array.from(Buffer.from('test data 2')),
		);
		await setTimeout(500); // Ensure time for data to process
		console.log('Reading data from session 2...');
		const output2 = await multiplexerHandle.readFromSession(session2);
		console.log('Output from session 2:', output2);
		t.is(
			output2.trim(),
			'test data 2',
			'Session 2 should receive the correct output',
		);

		// Remove session 1
		await multiplexerHandle.removeSession(session1);
		console.log('Removed session 1');
	} catch (error) {
		console.error('MultiplexerHandle operation error:', error);
		t.fail(`MultiplexerHandle operation failed: ${error.message}`);
	}
});

test('basic read operation', async (t) => {
	if (!t.context.ptyHandle) {
		t.fail('PtyHandle not available');
		return;
	}

	try {
		console.log('Starting basic read operation');
		const readPromise = t.context.ptyHandle.read();
		const timeoutPromise = setTimeout(TIMEOUT, 'Timeout');
		const result = await Promise.race([readPromise, timeoutPromise]);

		if (result === 'Timeout') {
			t.fail('Read operation timed out');
		} else {
			console.log('Read result:', result);
			t.pass('Read operation completed within timeout');
			t.is(typeof result, 'string', 'Read result should be a string');
		}
		console.log('Finished basic read operation');
	} catch (error) {
		console.error('Read operation error:', error);
		t.fail(`Read operation failed: ${error.message}`);
	}
});

test('basic write operation', async (t) => {
	if (!t.context.ptyHandle) {
		t.fail('PtyHandle not available');
		return;
	}

	try {
		console.log('Starting basic write operation');
		const writePromise = t.context.ptyHandle.write('echo "Hello, World!"');
		const timeoutPromise = setTimeout(TIMEOUT, 'Timeout');
		const result = await Promise.race([writePromise, timeoutPromise]);

		if (result === 'Timeout') {
			t.fail('Write operation timed out');
		} else {
			console.log('Write operation completed');
			t.pass('Write operation completed within timeout');
		}
		console.log('Finished basic write operation');
	} catch (error) {
		console.error('Write operation error:', error);
		t.fail(`Write operation failed: ${error.message}`);
	}
});

test('basic resize operation', async (t) => {
	if (!t.context.ptyHandle) {
		t.fail('PtyHandle not available');
		return;
	}

	try {
		console.log('Starting basic resize operation');
		const resizePromise = t.context.ptyHandle.resize(80, 24);
		const timeoutPromise = setTimeout(TIMEOUT, 'Timeout');
		const result = await Promise.race([resizePromise, timeoutPromise]);

		if (result === 'Timeout') {
			t.fail('Resize operation timed out');
		} else {
			console.log('Resize operation completed');
			t.pass('Resize operation completed within timeout');
		}
		console.log('Finished basic resize operation');
	} catch (error) {
		console.error('Resize operation error:', error);
		t.fail(`Resize operation failed: ${error.message}`);
	}
});

test('setting environment variables', async (t) => {
	if (!t.context.ptyHandle) {
		t.fail('PtyHandle not available');
		return;
	}

	try {
		const key = 'TEST_ENV';
		const value = '123';
		console.log('Starting environment variable setting');
		await t.context.ptyHandle.setEnv(key, value);
		console.log(`Environment variable ${key} set to ${value}`);

		// Retrieving status from MultiplexerHandle instead of PtyHandle directly
		const multiplexerHandle = t.context.ptyHandle.multiplexerHandle;
		const status = await multiplexerHandle.status();
		t.truthy(status, 'PTY status should be available');
		console.log('PTY Status:', status);

		t.pass('Environment variable setting completed');
	} catch (error) {
		console.error('Environment variable setting error:', error);
		t.fail(`Environment variable setting failed: ${error.message}`);
	}
});

test.after.always(() => {
	console.log(`Final PtyHandle instance count: ${ptyHandleCounter}`);
});
