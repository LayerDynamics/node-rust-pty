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
			nativeBinding = await import(`../darwin-${arch}/index.js`);
			break;
		case 'linux':
			nativeBinding = await import(`../linux-${arch}-gnu/index.js`);
			break;
		case 'win32':
			nativeBinding = await import(`../win32-${arch}-msvc/index.js`);
			break;
		default:
			throw new Error(`Unsupported platform: ${platform}`);
	}
} catch (error) {
	console.error(`Failed to import native binding: ${error}`);
	process.exit(1);
}

const { PtyHandle } = nativeBinding;

console.log('PtyHandle Imported:', PtyHandle);

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

test.after.always(() => {
	console.log(`Final PtyHandle instance count: ${ptyHandleCounter}`);
});
