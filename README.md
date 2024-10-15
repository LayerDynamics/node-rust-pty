# node-rust-pty

A high-performance, cross-platform PTY (pseudoterminal) implementation for Node.js, built with Rust and N-API.

## Table of Contents

- [Features](#features)
- [Installation](#installation)
- [Usage](#usage)
- [API Reference](#api-reference)
- [Platform Support](#platform-support)
- [Building from Source](#building-from-source)
- [Testing](#testing)
- [Benchmarks](#benchmarks)
- [Contributing](#contributing)
- [License](#license)

## Features

- Cross-platform support (macOS and Linux)
- High-performance Rust implementation
- Node.js bindings using N-API
- PTY multiplexing for managing multiple sessions
- Asynchronous I/O operations
- Configurable logging levels
- Environment variable management
- Shell changing capability
- Graceful shutdown and force kill options

## Installation

To install node-rust-pty, run the following command in your project directory:

```bash
npm install node-rust-pty
```

This package includes prebuilt binaries for supported platforms. If a prebuilt binary is not available for your platform, it will attempt to build from source during installation.

## Usage

Here's a basic example of how to use node-rust-pty:

```javascript
const { PtyHandle } = require('node-rust-pty');

async function main() {
  const pty = PtyHandle.new();
  const multiplexer = pty.multiplexerHandle;

  // Create a new session
  const sessionId = await multiplexer.createSession();

  // Write to the session
  await multiplexer.sendToSession(sessionId, Buffer.from('echo "Hello, PTY!"\n').toJSON().data);

  // Read from the session
  const output = await multiplexer.readFromSession(sessionId);
  console.log(output);

  // Close the PTY
  await pty.close();
}

main().catch(console.error);
```

## API Reference

### PtyHandle

The `PtyHandle` class represents the main PTY interface.

#### Static Methods

- `static new(): PtyHandle` - Creates a new PtyHandle instance.

#### Instance Methods

- `read(): Promise<string>` - Reads data from the PTY.
- `write(data: string): Promise<void>` - Writes data to the PTY.
- `resize(cols: number, rows: number): Promise<void>` - Resizes the PTY window.
- `execute(command: string): Promise<string>` - Executes a command in the PTY.
- `close(): Promise<void>` - Closes the PTY gracefully.
- `forceKill(forceTimeoutMs: number): Promise<void>` - Forcefully kills the PTY process.
- `pid(): Promise<number>` - Retrieves the process ID of the PTY.
- `killProcess(signal: number): Promise<void>` - Sends a signal to the PTY process.
- `waitpid(options: number): Promise<number>` - Waits for the PTY process to change state.
- `closeMasterFd(): Promise<void>` - Closes the master file descriptor of the PTY.
- `setEnv(key: string, value: string): Promise<void>` - Sets an environment variable for the PTY process.

#### Properties

- `multiplexerHandle: MultiplexerHandle` - Provides access to the MultiplexerHandle for managing PTY sessions.

### MultiplexerHandle

The `MultiplexerHandle` class manages multiple PTY sessions.

#### Static Methods

- `static new(): MultiplexerHandle` - Creates a new MultiplexerHandle instance.

#### Instance Methods

- `createSession(): Promise<number>` - Creates a new PTY session and returns its ID.
- `sendToSession(sessionId: number, data: Array<number>): Promise<void>` - Sends data to a specific session.
- `sendToPty(data: Uint8Array): Promise<void>` - Sends data directly to the PTY.
- `broadcast(data: Uint8Array): Promise<void>` - Broadcasts data to all active sessions.
- `readFromSession(sessionId: number): Promise<string>` - Reads data from a specific session.
- `readAllSessions(): Promise<Array<SessionData>>` - Reads data from all active sessions.
- `removeSession(sessionId: number): Promise<void>` - Removes a specific session.
- `mergeSessions(sessionIds: Array<number>): Promise<void>` - Merges multiple sessions into one.
- `splitSession(sessionId: number): Promise<SplitSessionResult>` - Splits a session into two separate sessions.
- `setEnv(key: string, value: string): Promise<void>` - Sets an environment variable for the PTY process.
- `changeShell(shellPath: string): Promise<void>` - Changes the shell of the PTY process.
- `status(): Promise<string>` - Retrieves the current status of the PTY process.
- `setLogLevel(level: string): Promise<void>` - Sets the logging level for the PTY process.
- `closeAllSessions(): Promise<void>` - Closes all active sessions.
- `shutdownPty(): Promise<void>` - Gracefully shuts down the PTY process and closes all sessions.

### Interfaces

#### SplitSessionResult

```typescript
interface SplitSessionResult {
  session1: number;
  session2: number;
}
```

#### SessionData

```typescript
interface SessionData {
  sessionId: number;
  data: string;
}
```

### PtyMultiplexer

The `PtyMultiplexer` class is an alternative implementation of the multiplexer functionality.

#### Constructor

- `constructor(ptyProcess: object)`

#### Methods

- `createSession(): number`
- `sendToSession(sessionId: number, data: Buffer): void`
- `broadcast(data: Buffer): void`
- `readFromSession(sessionId: number): Buffer`
- `readAllSessions(): Array<SessionData>`
- `removeSession(sessionId: number): void`
- `mergeSessions(sessionIds: Array<number>): void`
- `splitSession(sessionId: number): number[]`
- `setEnv(key: string, value: string): void`
- `changeShell(shellPath: string): void`
- `status(): string`
- `setLogLevel(level: string): void`
- `closeAllSessions(): void`
- `shutdownPty(): void`
- `forceShutdownPty(): void`
- `distributeOutput(): Promise<void>`
- `readFromPty(): Promise<Buffer>`
- `dispatchOutput(output: Buffer): Promise<void>`

Note: The `PtyMultiplexer` class provides similar functionality to `MultiplexerHandle` but with some differences in method signatures and return types.

## Platform Support

node-rust-pty currently supports the following platforms:

- macOS (x64, arm64)
- Linux (x64)

Support for Windows is planned for future releases.

## Building from Source

To build node-rust-pty from source, you'll need:

- Rust (latest stable version)
- Node.js (v10 or later)
- Python (for node-gyp)
- A C++ compiler (gcc, clang, or MSVC)

Follow these steps:

1. Clone the repository:
   ```bash
   git clone https://github.com/layerdynamics/node-rust-pty.git
   cd node-rust-pty
   ```

2. Install dependencies:
   ```bash
   npm install
   ```

3. Build the project:
   ```bash
   npm run build
   ```

## Testing

To run the test suite:

```bash
npm test
```

This will run both JavaScript and Rust tests. The test suite uses AVA for JavaScript tests and the built-in Rust test framework.

## Benchmarks

Benchmarks are available to measure the performance of key operations. To run the benchmarks:

```bash
npm run bench
```

## Contributing

Contributions to node-rust-pty are welcome! Please follow these steps:

1. Fork the repository
2. Create a new branch for your feature or bug fix
3. Write tests for your changes
4. Implement your changes
5. Run the test suite and ensure all tests pass
6. Submit a pull request with a clear description of your changes

Please adhere to the existing code style and add appropriate documentation for new features.

## License

This is free and unencumbered software released into the public domain.

Anyone is free to copy, modify, publish, use, compile, sell, or
distribute this software, either in source code form or as a compiled
binary, for any purpose, commercial or non-commercial, and by any
means.

In jurisdictions that recognize copyright laws, the author or authors
of this software dedicate any and all copyright interest in the
software to the public domain. We make this dedication for the benefit
of the public at large and to the detriment of our heirs and
successors. We intend this dedication to be an overt act of
relinquishment in perpetuity of all present and future rights to this
software under copyright law.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
OTHER DEALINGS IN THE SOFTWARE.

For more information, please refer to <http://unlicense.org/>

---

Created and maintained by [@layerdynamics](https://github.com/layerdynamics)